# Сборка и проверки

## Поддерживаемая цель

Текущий desktop-релиз — только Windows x64, NSIS. OCR и read-only scanner входят в Windows-пакет. Ресурсы и установщики для других платформ не поддерживаются.

## Требования

- Rust не ниже версии из `Cargo.toml`;
- Node.js и pnpm версии, совместимой с корневым `package.json`;
- .NET 8 SDK для self-contained OCR;
- Windows build tools, необходимые Rust и Tauri.

Зависимости устанавливаются один раз:

```text
pnpm install --frozen-lockfile
```

## Быстрый запуск

```text
START_PLATSCOPE_DEV.bat
```

Bat-файл проверяет основные зависимости, освобождает только занятый dev-порт PlatScope и запускает `cargo tauri dev`. Полный NSIS при этом не собирается.

Ручной эквивалент полного desktop-запуска:

```text
cargo tauri dev --config apps/desktop/src-tauri/tauri.conf.json
```

Tauri сам запускает frontend через `beforeDevCommand`. Для изолированной проверки
интерфейса в браузере можно отдельно выполнить `pnpm --dir apps/desktop dev`.

## Локальный quality gate

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
```

`pnpm build` публикует OCR, выполняет русский OCR self-test и собирает Svelte. Rust build script синхронизирует актуальные third-party notices при любом Cargo/Tauri build.

Перед выпуском и только по отдельной явной команде владельца дополнительно
проверяется release-конфигурация без упаковки установщика:

```text
cargo tauri build --no-bundle --config apps/desktop/src-tauri/tauri.conf.json
```

## Генерируемые каталоги

В Git не добавляются:

- `target`;
- `node_modules`;
- `dist`, `bin`, `obj` и coverage;
- `apps/desktop/src-tauri/resources/reward-ocr`;
- сгенерированная bundle-копия `THIRD_PARTY_NOTICES.md`;
- локальные БД, логи и signing secrets.

`apps/desktop/src-tauri/resources/reward-ocr/README.txt` остаётся единственным placeholder: он сохраняет ресурсный путь в чистом clone до первой OCR-сборки.

## Упаковка

Релизная упаковка запускается только по отдельной явной команде владельца:

```text
cargo tauri build --bundles nsis --config apps/desktop/src-tauri/tauri.conf.json
```

Результат находится в `target/release/bundle/nsis`. Подпись updater создаётся только при наличии локальных `TAURI_SIGNING_PRIVATE_KEY` и `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Проверка готовых локальных файлов:

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/verify-release-artifacts.ps1 -Mode Qa
```

Подробный порядок публикации описан в `RELEASE_SIGNING.md`.

## GitHub

GitHub используется как хранилище исходников и уже проверенных локальных релизных файлов. GitHub Actions в проекте отсутствуют: сборка, OCR, тесты, подпись, checksum и контрольная загрузка выполняются на рабочей машине.
