# PlatScope

PlatScope — локальное desktop-приложение для анализа рынка и инвентаря Warframe. Его цель — не повторить таблицу объявлений, а объяснимо ответить: что имеет смысл продавать, по какой цене, насколько рекомендация надёжна и почему.

Foundation, bulk ingestion, provider-neutral Pricing Engine, Market Browser, Live Pricing, History, встроенное read-only получение инвентаря, безопасный JSON import, Dashboard, Sell Now, аналитика Sets / Relics / Ducats / Riven disposition, полноценная локальная диагностика, русский/английский интерфейс и opt-in интеграция аккаунта WFM реализованы. Приложение умеет читать инвентарь запущенного PC-клиента Warframe по схеме TennoWorth, автоматически проверять свежесть market/metadata LKG, выбирать рыночную платформу, настраивать crossplay, bulk interval и live TTL, восстанавливать локальные рабочие фильтры и сортировку, показывать активные WFM top orders, отдельные nominal inventory/sellable values и слабую ликвидность, находить предмет по имени или canonical slug, показывать его точное требование мастерства из WFCD, показывать историю 7/30/90 дней, локально сопоставлять инвентарь с точным rank/subtype-вариантом, строить объяснимую очередь продажи, сравнивать Prime set с деталями, relic EV и plat/ducat, искать weapon disposition без ложной оценки уникального Riven roll, показывать состояние каждого источника без terminal logs, экспортировать безопасный allowlist-report, а после явного подключения — читать и вручную изменять собственные WFM listings. Daily bulk-снимок остаётся PC-only: для других платформ PC-цены не подставляются, а точные цены доступны только через явный live-запрос WFM.

Read-only scanner адаптирован из MIT-проекта TennoWorth. Он запрашивает только права чтения процесса, извлекает активные `accountId`/`nonce`, получает JSON непосредственно от Digital Extremes и не сохраняет credentials или raw memory. Метод не имеет официальной гарантии Digital Extremes и используется на риск игрока.

## Основные принципы

- реальные сделки важнее единичных объявлений;
- отсутствие надёжной цены представляется как `null`, а не как `0p` или `1p`;
- rank, subtype, platform и звёзды Ayatan входят в идентичность рыночного варианта;
- внешние JSON никогда не становятся внутренней моделью приложения;
- последний валидный локальный snapshot важнее повреждённого обновления;
- инвентарь остаётся на устройстве, telemetry по умолчанию отсутствует;
- каждая рекомендация должна объяснять источники, свежесть, ликвидность и confidence.

## Документация

- [Исследование](docs/RESEARCH.md)
- [Архитектура](docs/ARCHITECTURE.md)
- [Источники данных](docs/DATA_SOURCES.md)
- [Pricing Engine](docs/PRICING.md)
- [Market Browser](docs/MARKET_BROWSER.md)
- [Live Pricing](docs/LIVE_PRICING.md)
- [История и тренды](docs/HISTORY.md)
- [Инвентарь](docs/INVENTORY.md)
- [Уведомления о стороннем коде](THIRD_PARTY_NOTICES.md)
- [Аудит готовности по master-guideline](docs/COMPLETION_AUDIT.md)
- [Dashboard](docs/DASHBOARD.md)
- [Sell Now](docs/SELL_NOW.md)
- [Метаданные и аналитика](docs/METADATA_INSIGHTS.md)
- [Аккаунт WFM](docs/WFM_ACCOUNT.md)
- [Диагностика](docs/DIAGNOSTICS.md)
- [Локализация интерфейса](docs/LOCALIZATION.md)
- [Производительность](docs/PERFORMANCE.md)
- [Сборка и CI](docs/BUILD.md)
- [Подпись и доверенный релиз](docs/RELEASE_SIGNING.md)
- [Безопасность](docs/SECURITY.md)
- [Дорожная карта](docs/ROADMAP.md)

## Планируемые команды проверки

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm --prefix apps/desktop run check
npm --prefix apps/desktop test -- --run
npm --prefix apps/desktop run build
cargo tauri build --no-bundle --config apps/desktop/src-tauri/tauri.conf.json
```

PlatScope не связан с Digital Extremes или Warframe.Market.
