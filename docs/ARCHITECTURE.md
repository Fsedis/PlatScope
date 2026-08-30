# Архитектура PlatScope

## Архитектурная цель

PlatScope — offline-first desktop-приложение с локальным хранилищем. Внешние сервисы заменяемы; доменная модель, Pricing Engine и UI не знают их JSON-форматов.

```text
External providers
  -> bounded download
  -> provider parser
  -> schema and semantic validation
  -> normalized domain model
  -> atomic SQLite/cache promotion
  -> Pricing Engine
  -> application services
  -> Tauri commands/events
  -> Svelte presentation
```

## Инварианты

1. UI не получает raw upstream JSON.
2. Pricing Engine не зависит от конкретного provider ID.
3. `source_date`, `fetched_at` и `promoted_at` — разные понятия.
4. Snapshot публикуется только целиком после validation и транзакции.
5. Price snapshot и item catalog имеют независимый lifecycle.
6. Неуспешное обновление не изменяет last-known-good данные.
7. Rank, subtype, platform, `amber_stars` и `cyan_stars` являются частью `MarketVariantKey`.
8. Неизвестная цена представляется отсутствием значения.
9. Network, parsing и SQLite не выполняются в UI thread.
10. Sensitive inventory/auth data не пересекает telemetry boundary, потому что telemetry отсутствует по умолчанию.
11. Sell Priority является относительным ranking score, а не прогнозом платины в день; номинальная стоимость хранится и показывается отдельно.
12. Game metadata имеет независимый от price snapshot lifecycle; его сбой не блокирует pricing и не меняет metadata LKG. Riven weapon disposition хранится здесь же, но не входит в item Pricing Engine.
13. Bulk refresh запускается после offline-first startup и периодически проверяет срок LKG; ручной и background path объединены общим service lock, а UI-событие отправляется только после atomic promotion.
14. WFCD metadata проверяется тем же неблокирующим scheduler с отдельным 24-часовым TTL и lock; открытая аналитика перечитывает LKG только после `game-metadata-updated`.
13. Отсутствующая цена relic reward не заменяется `1p`, а уменьшает явно показанное покрытие EV.
14. WFM account auth изолирован от public market fetching; отсутствие credential никогда не блокирует pricing.
15. Локализация изменяет только presentation: canonical ID, slug и `MarketVariantKey` не зависят от языка.
15. Каждый account write требует явного confirmation на service boundary независимо от поведения UI.
16. Диагностический экран читает сохранённые факты и не превращает открытие UI в network health-check.
17. Рабочие фильтры и сортировка — локальное presentation state: Market и единый экран «Мои предметы» имеют отдельные versioned WebView keys, проходят allowlist-validation и никогда не включают поисковые строки.
18. «Мои предметы» получает единый DTO со всеми строками инвентаря; режимы полного списка, продажи и проверки являются локальными фильтрами, а не отдельными экранами или повторными provider-запросами.
19. «Торговая смена» является очередью решений внутри Market, а не дубликатом account orders: исправные ордера сворачиваются, пакет всегда показывает diff, а событие `EE.log` не становится account write без явного подтверждения.
20. Торговый диалог считается состоявшимся только после success-маркера игры; неоднозначный предмет, ранговый вариант или несовместимый `perTrade` никогда не сопоставляются догадкой.

## Структура workspace

```text
PlatScope/
  Cargo.toml
  crates/
    platscope-domain/       # provider-neutral types and invariants
    platscope-providers/    # provider ports and typed provider errors
    platscope-pricing/      # pure Pricing Engine, fixtures and explanations
    platscope-trends/       # pure historical trend and timing calculations
    platscope-inventory/    # bounded import, resolver and reserve calculations
    platscope-insights/     # pure set/relic/ducat calculations
    platscope-account/      # bounded WFM auth/orders client и OS keychain
    platscope-selling/      # pure bounded Sell Priority и nominal value
    platscope-storage/      # SQLite, migrations, repositories
    platscope-core/         # settings, logging, application services
  apps/
    desktop/
      src/                  # Svelte presentation only
      src-tauri/            # Tauri composition root and commands
  migrations/               # ordered SQLite migrations
  fixtures/                 # offline parser/pricing golden fixtures
  docs/
```

Допустимое направление зависимостей:

```text
domain <- providers
domain <- pricing
domain <- trends
domain <- inventory
domain <- insights
domain/trends <- selling
domain <- storage
domain/providers/pricing/trends/inventory/insights/selling/storage/account <- core
core <- desktop Tauri shell
```

Frontend зависит только от сериализуемых command DTO. Rust crates никогда не зависят от frontend.

## Модули

### `platscope-domain`

Стабильные типы: `MarketVariantKey`, `MarketRecord`, `NormalizedMarketSnapshot`, `SnapshotMetadata`, `PriceConfidence`, `Freshness`, inventory identities. Здесь нет HTTP, SQLite и Tauri.

### `platscope-providers`

Порты `BulkMarketProvider`, `LiveMarketProvider`, `InventoryProvider`, `MetadataProvider` и типизированные ошибки. Каждый адаптер владеет транспортным DTO и parser. Ошибка schema drift получает код `UPSTREAM_SCHEMA_CHANGED`.

Catalog следует отдельному `MetadataProvider`; production-адаптер WFM v2 поставляет английские/русские имена и локализованные изображения, а relics.run остаётся независимым price provider. Exact component art из WFCD перекрывает общий WFM thumbnail только на уровне presentation DTO.

### `platscope-storage`

SQLite — derived state и диагностируемый индекс. Все изменения схемы идут через migrations. Репозиторий открывает соединение, включает foreign keys, задаёт busy timeout и выполняет migrations транзакционно.

### `platscope-pricing`

Чистый deterministic Pricing Engine. Получает только нормализованные `MarketRecord`, точный `MarketVariantKey` и опциональный `LiveOrderBook`; не зависит от HTTP, SQLite, Tauri и форматов внешних providers. Результат содержит отдельные fair/list/quick/lowest/depth цены, confidence, freshness и структурированные причины.

### `platscope-trends`

Чистый модуль расчёта исторических метрик и timing-сигнала. Получает компактные дневные точки точного `MarketVariantKey`, отбрасывает дни с недостаточным объёмом, рассчитывает 7/30/90-дневные медианы, изменения и средний объём. `PEAK` допускается только при подтверждении актуальным live-рынком.

### `platscope-inventory`

Чистый bounded parser PlatScope JSON v1, canonical resolver и расчёт резерва. Display name не используется как join key; exact rank/subtype никогда не заменяется другим tier. Unknown tradeability и unresolved variant дают нулевое sellable quantity. Application DTO дополнительно обогащается локальными `VaultStatus` из normalized metadata LKG и фактом наличия bulk `fair_price`; это не меняет сохранённый inventory snapshot и не обращается к provider network.

### `platscope-readonly-scan`

Изолированный MIT-модуль TennoWorth для поиска Warframe, read-only обхода памяти и получения inventory JSON от Digital Extremes. Это единственный crate с разрешённым `unsafe` FFI на Windows; права процесса ограничены query/read. Секреты сессии не пересекают Tauri command boundary.

### `platscope-selling`

Чистый модуль ранжирования кандидатов. Насыщает quantity, price и liquidity, применяет confidence/timing как ограничивающие множители и возвращает score с объяснениями. Номинальная стоимость рассчитывается отдельной функцией и не влияет на смысл score как относительной очереди.

### `platscope-insights`

Чистые вычисления для Prime sets, relic EV и ducat efficiency. Модуль принимает только нормализованные recipe quantities, exact inventory quantities и результаты Pricing Engine. Он не зависит от HTTP, SQLite или Tauri. Set-vs-parts использует confidence/liquidity-adjusted value, relic EV не нормализует partial coverage до 100%, а plat/ducat доступен только при credible fair price.

### `platscope-account`

Изолированный opt-in адаптер WFM account API. Владеет одноразовым legacy sign-in, актуальными v2 profile/order DTO, limiter, 2 MiB response limit и `CredentialStore`. Token доступен только внутри crate, zeroize-ится и сохраняется через OS keychain. Модуль не зависит от catalog, Pricing Engine, inventory или SQLite.

### `platscope-core`

Composition-independent application services: settings, logging, health, `MarketDataService`, `PricingService`, `MarketBrowserService`, `LivePricingService`, `HistoryService`, `InventoryService`, `SellNowService`, `GameMetadataService`, `InsightsService` и `AccountService`. Fallback orchestration, TTL live cache, фоновый импорт истории, metadata/inventory import orchestration, соединение inventory/pricing/trends/insights, обязательное подтверждение account writes и построение DTO находятся здесь, не в Tauri commands и не в UI. `SellNowSummary` разделяет номинал всех resolved owned-копий и номинал sellable-копий. `source_health` обновляется только после реальной provider-попытки и читается отдельным локальным diagnostics command.

### Desktop shell

Все синхронные Tauri-команды, которые читают или изменяют SQLite, принудительно используют асинхронный command context. Ожидание общей блокировки базы и сериализация DTO выполняются вне потока окна: длительное обновление может временно задержать новый запрос к данным, но не должно останавливать навигацию, перерисовку или обработку ввода WebView. Сетевые refresh-команды и сканирование Warframe остаются асинхронными; блокирующий обход памяти дополнительно вынесен в `spawn_blocking`.

Tauri создаёт окна, определяет data directory `PlatScope`, открывает `platscope.db`, строит сервисы и выдаёт узкие commands. Svelte показывает локальное состояние сразу; единый экран «Мои предметы» читает `sell_now`, который включает metadata, summary и все строки текущего inventory LKG, `search_market` читает только текущий рыночный LKG, refresh запускается в фоне и возвращает новое command state, а `diagnostics_status` не обращается в сеть и возвращает только безопасные агрегаты SQLite.

Один tailer `EE.log` обслуживает маркеры relic rewards и подтверждённые торговые диалоги. Торговый parser — чистая state machine с 120-секундным timeout; SQLite получает событие только после success-маркера, а UI перечитывает журнал по `trade-detected`. WFM write остаётся отдельной подтверждаемой командой. Подробный контракт: [Торговая смена](TRADING_SHIFT.md).

Отдельный opt-in companion poller принадлежит desktop filesystem boundary. Он читает только явно сохранённый абсолютный путь, требует стабильные size/mtime в двух проходах, memoize-ит попытку для неизменившегося sample и передаёт данные в `InventoryService::import_companion_json`. Core повторно проверяет `InventorySource::OverwolfCompanion` до SQLite promotion; UI получает только событие перечитать локальный LKG.

Язык читается и сохраняется через существующие команды настроек. `i18n.ts` держит типизированную UI-локаль в Svelte context; после её смены presentation повторно читает локальные DTO, чтобы получить нужные display names. Стабильные reason codes и типизированные факторы отделяют локализацию объяснений от расчётов Pricing Engine, Sell Priority и Insights.

`viewPreferences.ts` отвечает только за локальные рабочие представления. Каждый экран имеет отдельный versioned key; неизвестные enum-значения, повреждённый JSON и неподдерживаемая версия сбрасываются к безопасным defaults. Ошибка или запрет WebView storage не блокирует экран. Свободный поисковый ввод намеренно не сохраняется.

## Жизненный цикл запуска

1. Определить локальный data directory без network.
2. Инициализировать redacted structured logging.
3. Открыть SQLite и применить migrations.
4. Прочитать settings и LKG metadata.
5. Показать окно с локальным состоянием.
6. В фоновой задаче проверить catalog и market snapshot.
7. Независимо запустить bounded bootstrap истории: не более 7 отсутствующих дней за один запуск.
8. Скачать response в bounded temporary file/body.
9. Распарсить, нормализовать, проверить размер/shape/count/date/finite numbers.
10. В одной транзакции импортировать derived state и metadata.
11. Атомарно promoted snapshot становится текущим; UI получает новое immutable view.

При сбое фоновых шагов приложение продолжает использовать LKG и показывает причину/возраст. Ошибка одного исторического дня не блокирует запуск и не удаляет уже импортированную историю.

## SQLite

Начальный набор таблиц:

```text
schema_migrations
settings
market_snapshots
market_prices
item_catalog
source_health
market_history_snapshots
market_history
inventory_snapshots
inventory_items
game_metadata_snapshots
```

Миграция 4 добавляет к каталогу опциональное русское имя и нормализованный `search_text`. При обновлении каталога индекс строится из canonical slug, английского и доступного русского имени; миграция существующей базы сразу backfill-ит slug и английское имя.

Миграция 5 добавляет компактную историю рынка. Она сохраняет только дату, точный вариант, медианы закрытых/sell/buy предложений и объём закрытых сделок; текущий LKG snapshot backfill-ится как первая историческая точка.

Миграция 6 добавляет атомарный inventory LKG: метаданные исходного снимка и агрегированные exact rows с отдельными tradeable/untradeable/unknown/leveled quantities и resolution status.

Миграция 7 добавляет отдельный атомарный LKG игровых метаданных: нормализованные Prime set recipes, relic rewards/refinements, ducats и vault status сохраняются одним JSON snapshot с counts и SHA-256. Публикация не связана с price snapshot и откатывается целиком при нарушении инвариантов.

Миграция 8 добавляет Riven disposition count. Миграция 9 добавляет count определений предметов и компактную snapshot-scoped таблицу `game_item_definitions(slug, game_ref, mastery_requirement)`. Она публикуется в той же транзакции, что и metadata LKG, и позволяет Market Browser присоединять MR по exact canonical slug без разбора полного JSON.

Миграция 10 добавляет локальный журнал подтверждённых сделок `trade_events`: dedup fingerprint, время, платина, bounded JSON-списки предметов, статус сверки и снимок применённого изменения для отмены.

Миграция 11 добавляет read-only связи установленных модов: признак полного сканирования сборок в inventory snapshot, защищённые количества в inventory rows и нормализованные размещения по хешированному экземпляру предмета и конфигурации.

Позже отдельной migration добавляется `live_quotes`. Не создаём фиктивные таблицы до появления её репозиторного контракта.

## Конкурентность и отмена

- Provider API асинхронный.
- `LivePricingService` объединяет одновременные запросы одним async mutex, повторно использует quote в пределах TTL и при сетевом сбое может вернуть явно помеченный stale cache.
- Долгие операции принимают cancellation token или ограничиваются task lifetime.
- HTTP имеет connect/request timeout, общий body limit 32 MiB и централизованный rate limiter.
- SQLite writes сериализованы сервисным слоем; reads не удерживают UI.
- Повторный запрос одного live key объединяется в один in-flight request.
- Account requests сериализованы отдельным async mutex и выдерживают интервал 350 ms; account response ограничен 2 MiB.

## Решения foundation

- Tauri 2, Rust, Svelte 5, TypeScript, Vite, SQLite.
- `rusqlite` с bundled SQLite для воспроизводимой desktop-сборки.
- `tracing`/`tracing-subscriber` для structured logging.
- `serde` DTO на IPC boundary.
- Rust workspace — единая версия и единый lint policy.

Продуктовая навигация не входит в foundation: Market исследует рынок, «Мои предметы» обслуживают инвентарь и продажу, а специализированные расчёты остаются в аналитике.
