//! Ограниченное по времени выполнение одноразовых OCR-команд.

use std::fmt;
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

const MAX_STDOUT_BYTES: usize = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) enum ProcessFailure {
    Io(std::io::Error),
    TimedOut { pid: Option<u32> },
    OutputLimit(&'static str),
    Cleanup(String),
}

impl fmt::Display for ProcessFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Ошибка OCR-процесса: {error}"),
            Self::TimedOut { pid } => {
                write!(formatter, "Истекло время ожидания OCR-процесса {pid:?}")
            }
            Self::OutputLimit(stream) => {
                write!(formatter, "OCR-процесс превысил предел ответа {stream}")
            }
            Self::Cleanup(error) => write!(formatter, "Не удалось завершить OCR-процесс: {error}"),
        }
    }
}

impl From<std::io::Error> for ProcessFailure {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Тайм-аут охватывает передачу запроса, чтение обоих потоков и ожидание выхода.
/// Перед возвратом ошибки завершаем процесс и ждём его выхода. При отмене
/// вызывающей задачи `kill_on_drop` также не оставляет помощник в фоне.
pub(super) async fn run(
    command: &mut Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Output, ProcessFailure> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::hide_process_window(command.as_std_mut());
    let mut child = command.spawn()?;
    let pid = child.id();
    let result = tokio::time::timeout(timeout, collect_output(&mut child, input)).await;
    let result = result.unwrap_or(Err(ProcessFailure::TimedOut { pid }));
    if result.is_err() {
        stop(&mut child).await?;
    }
    result
}

async fn collect_output(
    child: &mut Child,
    input: Option<Vec<u8>>,
) -> Result<Output, ProcessFailure> {
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::other("Не удалось открыть стандартный вывод OCR-процесса")
    })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("Не удалось открыть поток ошибок OCR-процесса"))?;
    let write_input = async move {
        if let Some(input) = input {
            let mut stdin = stdin
                .ok_or_else(|| std::io::Error::other("Не удалось открыть вход OCR-процесса"))?;
            stdin.write_all(&input).await?;
        }
        Ok::<(), ProcessFailure>(())
    };
    // Читаем stdout/stderr одновременно с записью stdin: полный буфер одного
    // из потоков не должен блокировать передачу каталога или выход помощника.
    let ((), stdout, stderr, status) = tokio::try_join!(
        write_input,
        read_bounded(stdout, MAX_STDOUT_BYTES, "stdout"),
        read_bounded(stderr, MAX_STDERR_BYTES, "stderr"),
        async { child.wait().await.map_err(ProcessFailure::from) },
    )?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

async fn read_bounded(
    reader: impl AsyncRead + Unpin,
    limit: usize,
    stream: &'static str,
) -> Result<Vec<u8>, ProcessFailure> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(ProcessFailure::OutputLimit(stream));
    }
    Ok(bytes)
}

async fn stop(child: &mut Child) -> Result<(), ProcessFailure> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    match tokio::time::timeout(CLEANUP_TIMEOUT, child.kill()).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(ProcessFailure::Cleanup(error.to_string())),
        Err(_) => Err(ProcessFailure::Cleanup(
            "Истекло время ожидания завершения".into(),
        )),
    }
}
