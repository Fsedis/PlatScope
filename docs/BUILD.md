# Сборка и CI

## Локальный quality gate

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pnpm check
pnpm test
pnpm build
cargo tauri build --no-bundle --config apps/desktop/src-tauri/tauri.conf.json
```

Корневые `pnpm check`, `pnpm test` и `pnpm build` проверяют Tauri desktop. Версия pnpm закреплена в корневом `package.json`, зависимости frontend — в `pnpm-lock.yaml`, Rust — в `Cargo.lock`.

Общий локальный preflight:

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/verify-release-artifacts.ps1 -Mode Qa
```

`Qa` требует canonical executable, единственный NSIS и совпадающий desktop checksum. `NotSigned` и отсутствующий AppImage выводятся как `WARN`, а не скрываются. `Trusted` использует те же проверки fail-closed и дополнительно требует `Valid` Authenticode с timestamp, AppImage, checksum и проверяемую detached signature; на текущих QA artifacts он обязан завершаться ошибкой.

Локальная упаковка Windows:

```text
cargo tauri build --bundles nsis --config apps/desktop/src-tauri/tauri.conf.json
```

Результат: канонический executable `target/release/platscope.exe`, installer `target/release/bundle/nsis/PlatScope_<version>_x64-setup.exe` и updater-подпись `.sig`, если заданы `TAURI_SIGNING_PRIVATE_KEY` и `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Имя Rust package остаётся `platscope-desktop`, а явный binary target называется `platscope`. Authenticode status пока ожидаемо равен `NotSigned`, даже если пакет имеет отдельную подпись Tauri.

## GitHub Actions

Workflow `.github/workflows/quality.yml` запускается для push, pull request и вручную. Матрица содержит `windows-latest` и `ubuntu-22.04`; оба job выполняют одинаковые formatter/lint/test/frontend gates и явно собирают release binary `platscope` из package `platscope-desktop`.

Workflow имеет только `contents: read`, не публикует release, не загружает secrets и отменяет устаревший прогон той же ветки. Production network smoke-test остаётся `ignored`, поэтому CI детерминирован и не зависит от доступности upstream providers.

Отдельный ручной workflow `.github/workflows/package.yml` создаёт три workflow artifacts без GitHub Release и без write permission:

- Windows x64 — NSIS installer;
- Linux x86_64 — AppImage;

Workflow использует официальный `tauri-apps/tauri-action`, а относительный `projectPath` указывает на `apps/desktop`, где Tauri видит `src-tauri/tauri.conf.json` и выполняет закреплённый frontend build. После упаковки отдельный OS-specific шаг создаёт `SHA256SUMS.txt`; installer/AppImage и manifest загружаются одним QA artifact. Checksum подтверждает целостность относительно ожидаемого значения, но не заменяет подпись издателя.

Windows packaging job после создания checksum запускает `verify-release-artifacts.ps1 -Mode Qa`. Companion job загружает уже проверенный `dist` вместе с собственным `SHA256SUMS.txt`. Это остаётся artifact-level проверкой и не заменяет protected tag/environment, identity издателя или повторную проверку после публичной публикации.

Публичный workflow `.github/workflows/release.yml` запускается тегом `v<version>`, собирает Windows NSIS с защищёнными signing secrets и публикует GitHub Release, `.sig`, `latest.json` и `SHA256SUMS.txt`. Политика хранения ключей и различие между подписью updater и Authenticode описаны в [Подпись и публичный релиз](RELEASE_SIGNING.md).

## Linux prerequisites

Ubuntu job устанавливает WebKitGTK 4.1, Ayatana AppIndicator, librsvg, OpenSSL, xdo, build tools и утилиты упаковки из системного репозитория. Набор основан на официальных требованиях Tauri 2:

- <https://v2.tauri.app/start/prerequisites/>;
- <https://v2.tauri.app/distribute/pipelines/github/>.

`ubuntu-22.04` выбран как явная базовая система вместо плавающего `ubuntu-latest`: это делает минимальную glibc/WebKitGTK границу сборки более предсказуемой.

## Граница проверки

При успешном прогоне quality CI доказывает, что source tree компилируется и тестируется на Windows/Linux. Публичный релизный канал сейчас выпускает Windows NSIS; Linux AppImage остаётся QA artifact до отдельной проверки OCR-поведения и механизма подписи. `0.1.21` — первая версия со встроенным updater, поэтому её нужно установить вручную поверх `0.1.20`; дальнейшие версии доступны из интерфейса PlatScope.
