//! Статус WFM живёт в отдельном соединении и меняется только по команде пользователя.
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, protocol::WebSocketConfig,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async_with_config};

use crate::AccountToken;

const ENDPOINT: &str = "wss://ws.warframe.market/socket";
const AUTH: &str = "@wfm|cmd/auth/signIn";
const SET: &str = "@wfm|cmd/status/set";
const EVENT: &str = "@wfm|event/status/set";
const CONNECTION_ERROR: &str = "Связь с Warframe Market потеряна. Восстанавливаем соединение…";
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type Reply = oneshot::Sender<Result<PresenceView, String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketPresence {
    Online,
    Ingame,
    #[serde(alias = "offline")]
    Invisible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceConnection {
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceView {
    pub connection: PresenceConnection,
    pub status: Option<MarketPresence>,
    pub status_until: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl Default for PresenceView {
    fn default() -> Self {
        Self {
            connection: PresenceConnection::Connecting,
            status: None,
            status_until: None,
            error: None,
        }
    }
}

struct Change {
    status: MarketPresence,
    reply: Reply,
}
struct Session {
    changes: mpsc::Sender<Change>,
    view: watch::Receiver<PresenceView>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub struct PresenceService {
    session: Mutex<Option<Session>>,
}

impl PresenceService {
    /// Запускает синхронизацию без изменения статуса на сервере.
    pub async fn view(&self, token: AccountToken) -> PresenceView {
        let mut session = self.session.lock().await;
        if session
            .as_ref()
            .is_some_and(|active| active.task.is_finished())
        {
            session.take();
        }
        let active = session.get_or_insert_with(|| {
            let (changes, receiver) = mpsc::channel(1);
            let (sender, view) = watch::channel(PresenceView::default());
            let task = tokio::spawn(run(token, receiver, sender, ENDPOINT.to_owned()));
            Session {
                changes,
                view,
                task,
            }
        });
        active.view.borrow().clone()
    }

    /// Меняет статус и ждёт подтверждения именно этой команды.
    ///
    /// # Errors
    /// Возвращает понятную ошибку при отсутствии связи или отклонении сервером.
    pub async fn set(&self, status: MarketPresence) -> Result<PresenceView, String> {
        let receiver = {
            let session = self.session.lock().await;
            let active = session
                .as_ref()
                .ok_or("Дождитесь загрузки статуса Warframe Market.")?;
            if active.view.borrow().connection != PresenceConnection::Connected {
                return Err("Дождитесь восстановления связи с Warframe Market.".into());
            }
            let (reply, receiver) = oneshot::channel();
            active
                .changes
                .try_send(Change { status, reply })
                .map_err(|_| "Предыдущее изменение статуса ещё выполняется.")?;
            receiver
        };
        timeout(Duration::from_secs(20), receiver)
            .await
            .map_err(|_| "Warframe Market не подтвердил изменение. Проверьте текущий статус.")?
            .map_err(|_| "Связь с Warframe Market прервана. Проверьте текущий статус.".to_owned())?
    }

    /// Закрывает соединение при выходе из аккаунта, отменяя ожидающие команды.
    pub async fn stop(&self) {
        self.session.lock().await.take();
    }
}

async fn run(
    token: AccountToken,
    mut changes: mpsc::Receiver<Change>,
    view: watch::Sender<PresenceView>,
    endpoint: String,
) {
    let mut delay = 1;
    loop {
        let started = Instant::now();
        let result = connection(&token, &mut changes, &view, &endpoint).await;
        // Ни выбранный статус, ни неподтверждённая команда не повторяются при переподключении.
        view.send_modify(|state| {
            state.connection = PresenceConnection::Reconnecting;
            state.status = None;
            state.status_until = None;
            state.error = Some(result.err().unwrap_or_else(|| CONNECTION_ERROR.into()));
        });
        while let Ok(change) = changes.try_recv() {
            let _ = change.reply.send(Err(CONNECTION_ERROR.into()));
        }
        if changes.is_closed() {
            return;
        }
        if started.elapsed() > Duration::from_secs(60) {
            delay = 1;
        }
        tokio::time::sleep(Duration::from_secs(delay)).await;
        delay = (delay * 2).min(30);
    }
}

#[derive(Deserialize)]
struct Envelope {
    route: String,
    #[serde(default)]
    payload: Value,
    id: Option<String>,
    meta: Option<Meta>,
}
#[derive(Deserialize)]
struct Meta {
    stream: String,
    revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RichStatus {
    status: MarketPresence,
    status_until: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct StatusStream {
    cursor: Option<(String, u64)>,
}
impl StatusStream {
    fn apply(
        &mut self,
        message: &Envelope,
        view: &watch::Sender<PresenceView>,
    ) -> Result<bool, String> {
        let meta = message
            .meta
            .as_ref()
            .ok_or("Warframe Market не прислал номер обновления статуса.")?;
        if !meta.stream.starts_with("status:") {
            return Err("Неизвестный поток статуса Warframe Market.".into());
        }
        if let Some((stream, revision)) = &self.cursor {
            if stream != &meta.stream {
                return Err("Аккаунт статуса Warframe Market изменился.".into());
            }
            if meta.revision <= *revision {
                return Ok(false);
            }
        }
        let status: RichStatus = serde_json::from_value(message.payload.clone())
            .map_err(|_| "Warframe Market прислал неизвестный статус.")?;
        self.cursor = Some((meta.stream.clone(), meta.revision));
        view.send_modify(|state| {
            state.status = Some(status.status);
            state.status_until = status.status_until;
        });
        Ok(true)
    }
}

async fn send(socket: &mut Socket, message: Message) -> Result<(), String> {
    timeout(Duration::from_secs(5), socket.send(message))
        .await
        .map_err(|_| CONNECTION_ERROR.to_owned())?
        .map_err(|_| CONNECTION_ERROR.to_owned())
}

struct Pending {
    id: String,
    change: Change,
    sent: Instant,
}

async fn open_socket(token: &AccountToken, endpoint: &str) -> Result<Socket, String> {
    let mut request = endpoint
        .into_client_request()
        .map_err(|_| CONNECTION_ERROR)?;
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "wfm".parse().map_err(|_| CONNECTION_ERROR)?,
    );
    let config = WebSocketConfig::default()
        .max_message_size(Some(64 * 1024))
        .max_frame_size(Some(64 * 1024));
    let (mut socket, _) = timeout(
        Duration::from_secs(10),
        connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| CONNECTION_ERROR)?
    .map_err(|_| CONNECTION_ERROR)?;
    send(
        &mut socket,
        Message::Text(
            json!({"route":AUTH,"id":"auth","payload":{"token":token.expose()}})
                .to_string()
                .into(),
        ),
    )
    .await?;
    Ok(socket)
}

async fn connection(
    token: &AccountToken,
    changes: &mut mpsc::Receiver<Change>,
    view: &watch::Sender<PresenceView>,
    endpoint: &str,
) -> Result<(), String> {
    let mut socket = open_socket(token, endpoint).await?;
    let mut stream = StatusStream::default();
    let mut authorized = false;
    let mut sequence = 0_u64;
    let mut pending: Option<Pending> = None;
    let started = Instant::now();
    let mut last_received = started;
    let mut last_ping = started;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let result = async {
        loop {
            let ready = authorized && stream.cursor.is_some();
            tokio::select! {
                change = changes.recv(), if ready && pending.is_none() => {
                    let Some(change) = change else { return Ok(()); };
                    if change.reply.is_closed() { continue; }
                    sequence += 1;
                    let id = format!("status-{sequence}");
                    let message = json!({"route":SET,"id":id,"payload":{"status":change.status,"duration":null}});
                    pending = Some(Pending { id, change, sent: Instant::now() });
                    send(&mut socket, Message::Text(message.to_string().into())).await?;
                }
                message = socket.next() => {
                    let message = message.ok_or(CONNECTION_ERROR)?.map_err(|_| CONNECTION_ERROR)?;
                    last_received = Instant::now();
                    let text = match message {
                        Message::Text(text) => text,
                        Message::Ping(payload) => { send(&mut socket, Message::Pong(payload)).await?; continue; }
                        Message::Close(_) => return Err(CONNECTION_ERROR.to_owned()),
                        _ => continue,
                    };
                    let message: Envelope = serde_json::from_str(&text).map_err(|_| "Некорректный ответ статуса Warframe Market.")?;
                    if message.route == format!("{AUTH}:ok") && message.id.as_deref() == Some("auth") { authorized = true; }
                    else if message.route == EVENT || (message.route == format!("{SET}:ok") && pending.as_ref().is_some_and(|p| message.id.as_ref() == Some(&p.id))) {
                        stream.apply(&message, view)?;
                    } else if message.route == format!("{AUTH}:error") {
                        return Err("Warframe Market отклонил вход. Подключите аккаунт заново.".into());
                    }
                    if authorized && stream.cursor.is_some() {
                        view.send_modify(|state| { state.connection = PresenceConnection::Connected; state.error = None; });
                    }
                    if pending.as_ref().is_some_and(|p| message.id.as_ref() == Some(&p.id)) {
                        if message.route == format!("{SET}:ok") {
                            let request = pending.take().expect("pending matched");
                            let state = view.borrow().clone();
                            let response = if state.status == Some(request.change.status) { Ok(state) }
                                else { Err("Статус уже изменён другим подключением. Показано состояние сервера.".into()) };
                            let _ = request.change.reply.send(response);
                        } else if message.route.ends_with(":error") || message.route.contains("/error") {
                            let request = pending.take().expect("pending matched");
                            let _ = request.change.reply.send(Err("Warframe Market отклонил изменение статуса. Проверьте подтверждение аккаунта и повторите попытку.".into()));
                        }
                    }
                }
                _ = tick.tick() => {
                    if (!ready && started.elapsed() > Duration::from_secs(12)) || last_received.elapsed() > Duration::from_secs(65) {
                        return Err(CONNECTION_ERROR.into());
                    }
                    if pending.as_ref().is_some_and(|p| p.sent.elapsed() > Duration::from_secs(12)) {
                        return Err("Warframe Market не подтвердил изменение. Перепроверяем статус…".into());
                    }
                    if last_ping.elapsed() >= Duration::from_secs(20) {
                        send(&mut socket, Message::Ping(Vec::new().into())).await?;
                        last_ping = Instant::now();
                    }
                }
            }
        }
    }.await;
    if let Some(request) = pending {
        let _ = request.change.reply.send(Err(result
            .clone()
            .err()
            .unwrap_or_else(|| CONNECTION_ERROR.into())));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::handshake::server::{Request, Response},
    };

    type TestSocket = WebSocketStream<TcpStream>;

    #[allow(clippy::result_large_err)] // Тип ошибки задан внешним API handshake.
    async fn accept(listener: &TcpListener) -> TestSocket {
        let (tcp, _) = listener.accept().await.unwrap();
        accept_hdr_async(tcp, |request: &Request, mut response: Response| {
            assert_eq!(request.headers()["Sec-WebSocket-Protocol"], "wfm");
            response
                .headers_mut()
                .insert("Sec-WebSocket-Protocol", "wfm".parse().unwrap());
            Ok(response)
        })
        .await
        .unwrap()
    }

    async fn read(socket: &mut TestSocket) -> Value {
        let message = timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    }
    fn status(route: &str, id: Option<&str>, revision: u64, value: &str) -> Value {
        json!({"route":route,"id":id,"payload":{"status":value,"statusUntil":null},"meta":{"stream":"status:test-user","revision":revision}})
    }
    async fn emit(socket: &mut TestSocket, message: Value) {
        socket
            .send(Message::Text(message.to_string().into()))
            .await
            .unwrap();
    }
    async fn ready(view: &mut watch::Receiver<PresenceView>) {
        timeout(Duration::from_secs(5), async {
            loop {
                if view.borrow_and_update().connection == PresenceConnection::Connected {
                    break;
                }
                view.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn stale_status_cannot_overwrite_newer_server_state() {
        let (view, receiver) = watch::channel(PresenceView::default());
        let mut stream = StatusStream::default();
        for (revision, value) in [(5, "ingame"), (4, "invisible"), (5, "online")] {
            stream
                .apply(
                    &serde_json::from_value(status(EVENT, None, revision, value)).unwrap(),
                    &view,
                )
                .unwrap();
        }
        assert_eq!(receiver.borrow().status, Some(MarketPresence::Ingame));
        let mut wrong_account = status(EVENT, None, 6, "invisible");
        wrong_account["meta"]["stream"] = json!("status:other-user");
        assert!(
            stream
                .apply(&serde_json::from_value(wrong_account).unwrap(), &view)
                .is_err()
        );
        assert_eq!(receiver.borrow().status, Some(MarketPresence::Ingame));
        stream
            .apply(
                &serde_json::from_value(status(EVENT, None, 6, "offline")).unwrap(),
                &view,
            )
            .unwrap();
        assert_eq!(receiver.borrow().status, Some(MarketPresence::Invisible));
    }

    #[tokio::test]
    async fn changes_require_ack_and_are_not_replayed_after_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}/socket", listener.local_addr().unwrap());
        let (changes, receiver) = mpsc::channel(1);
        let (sender, mut view) = watch::channel(PresenceView::default());
        let task = tokio::spawn(run(
            AccountToken::new("test-token").unwrap(),
            receiver,
            sender,
            endpoint,
        ));
        let service = PresenceService {
            session: Mutex::new(Some(Session {
                changes,
                view: view.clone(),
                task,
            })),
        };
        let server = tokio::spawn(async move {
            let mut socket = accept(&listener).await;
            assert_eq!(read(&mut socket).await["route"], AUTH);
            emit(
                &mut socket,
                json!({"route":format!("{AUTH}:ok"),"id":"auth"}),
            )
            .await;
            // Вход сам по себе не должен отправлять желаемый статус.
            assert!(
                timeout(Duration::from_millis(100), socket.next())
                    .await
                    .is_err()
            );
            emit(&mut socket, status(EVENT, None, 1, "invisible")).await;
            let request = read(&mut socket).await;
            assert_eq!(
                request["payload"],
                json!({"status":"ingame","duration":null})
            );
            emit(
                &mut socket,
                status(&format!("{SET}:ok"), Some("unrelated"), 2, "online"),
            )
            .await;
            emit(
                &mut socket,
                status(&format!("{SET}:ok"), request["id"].as_str(), 3, "ingame"),
            )
            .await;
            emit(&mut socket, status(EVENT, None, 2, "invisible")).await;
            let rejected = read(&mut socket).await;
            emit(
                &mut socket,
                json!({"route":format!("{SET}:error"),"id":rejected["id"],"payload":"denied"}),
            )
            .await;
            let lost = read(&mut socket).await;
            assert_eq!(lost["payload"]["status"], "invisible");
            socket.close(None).await.unwrap();
            let mut restored = accept(&listener).await;
            assert_eq!(read(&mut restored).await["route"], AUTH);
            emit(
                &mut restored,
                json!({"route":format!("{AUTH}:ok"),"id":"auth"}),
            )
            .await;
            emit(&mut restored, status(EVENT, None, 0, "online")).await;
            // Команда без подтверждения не отправляется после нового входа.
            assert!(
                timeout(Duration::from_millis(200), restored.next())
                    .await
                    .is_err()
            );
        });
        ready(&mut view).await;
        let confirmed = service.set(MarketPresence::Ingame).await.unwrap();
        assert_eq!(confirmed.status, Some(MarketPresence::Ingame));
        assert!(service.set(MarketPresence::Online).await.is_err());
        assert_eq!(view.borrow().status, Some(MarketPresence::Ingame));
        assert!(service.set(MarketPresence::Invisible).await.is_err());
        server.await.unwrap();
        assert_eq!(view.borrow().status, Some(MarketPresence::Online));
        service.stop().await;
        assert!(service.session.lock().await.is_none());
    }
}
