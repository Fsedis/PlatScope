# Безопасность

## Границы доверия

Недоверенные входы: HTTP responses, cache files, imports, SQLite из старой версии, game process memory и локальные логи. Все проходят size limits, parsing и semantic validation до попадания в доменную модель.

## Запрещённые действия

- запись в память Warframe;
- DLL/code injection и hooks;
- обход anti-cheat;
- remote code execution или загрузка исполняемого кода из provider;
- plaintext tokens/nonces;
- автоматическое размещение/изменение orders без явного действия пользователя;
- отправка inventory на сервер PlatScope;
- скрытая telemetry.

## Network

- HTTPS only;
- redirect policy не разрешает downgrade;
- response body limit 32 MiB;
- connect/request timeouts;
- централизованный rate limiter и bounded retries;
- identifiable User-Agent;
- downloaded JSON остаётся данными и никогда не исполняется;
- checksum фиксируется для диагностики, но не заменяет schema validation.

## Локальные данные

Database: `platscope.db` в системной data directory `PlatScope`. Запись snapshot выполняется через transaction и атомарную promotion. Temporary files создаются в той же filesystem, получают ограниченные права и удаляются после обработки.

WFM token хранится только через OS keychain/credential manager. Если secure storage недоступен, интеграция остаётся выключенной; fallback в plaintext запрещён. Email и password не сохраняются, password очищается после одноразового sign-in. SQLite содержит только несекретный device ID.

## WFM account

- account integration выключена до явного подключения;
- auth/orders client отделён от public market providers;
- legacy v1 sign-in используется только как опубликованный WFM переходный путь до публичного OAuth; first-party auth не имитируется;
- `AccountToken` не сериализуется, zeroize-ится и redacted в `Debug`;
- account body limit — 2 MiB, timeouts — 5/15 секунд, запросы сериализованы с интервалом 350 ms;
- create/update/delete отклоняются backend-сервисом без отдельного `confirmed: true`;
- disconnect всегда удаляет local credential, даже если remote sign-out недоступен.

## Inventory acquisition

Production build включает явное пользовательское read-only сканирование по MIT-реализации TennoWorth. Windows boundary запрашивает только query/read права и не использует write/injection; Linux boundary читает `/proc`. Найденные `accountId`/`nonce` применяются только для одного HTTPS-запроса inventory endpoint Digital Extremes, не возвращаются в WebView, не сохраняются и не логируются. Raw response проходит bounds/validation и не хранится после нормализации. Digital Extremes не гарантирует ban-safety сторонних memory readers, что прямо указано в UI.

## Logging

Structured logs используют allowlist полей. Запрещены request auth headers, nonce, raw inventory, memory fragments, full account IDs и WFM token. Ошибки upstream сохраняют status, provider, endpoint class, latency и redacted message.

Экран диагностики использует тот же allowlist: provider timestamps/status/latency/failure count и агрегаты локального покрытия. Credentials, account IDs, nonce и raw payload отсутствуют в IPC DTO. Открытие экрана не запускает сетевой health-check.

## Packaging и подпись

Ручной CI packaging создаёт workflow artifacts с read-only repository permission. Теговый release workflow получает отдельный Tauri updater key только через GitHub Secrets, публикует подписанный update artifact, `latest.json` и SHA-256. Приватный ключ не доступен frontend, обычным quality jobs и pull request из forks. Authenticode-сертификата пока нет, поэтому Windows может показывать SmartScreen; подпись updater не выдаётся за подпись подтверждённого Windows-издателя. Полная политика: [Подпись и публичный релиз](RELEASE_SIGNING.md).

Уровень локального логирования можно переопределить только через `PLATSCOPE_LOG`; глобальная переменная `RUST_LOG` других Rust-приложений не меняет диагностическую полноту PlatScope.

## Tauri boundary

- минимальный capability allowlist;
- нет произвольного shell execution из frontend;
- commands принимают валидируемые DTO;
- filesystem access скрыт за Rust services;
- CSP и navigation ограничены локальным приложением и явно разрешёнными ссылками;
- внешние ссылки открываются только после пользовательского действия.

## Dependency и supply chain

- lockfiles коммитятся;
- зависимости минимальны и проверяются audit-инструментами;
- GPL/proprietary reference code не переносится;
- updater проверяет встроенную подпись Tauri, требует подтверждения установки и не блокирует ручное восстановление;
- provider schema changes не могут автоматически менять исполняемый код.

## Diagnostic export

Экспорт создаётся локально и включает версии, health, redacted errors, schema/count/checksum metadata. Пользователь видит файл до отправки. Secrets и raw inventory исключены.
