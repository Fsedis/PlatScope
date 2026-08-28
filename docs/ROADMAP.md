# Дорожная карта

Работа идёт вертикальными срезами. Переход к следующему этапу разрешён после formatter, clippy, Rust tests, frontend typecheck/tests и build.

## 1. Foundation

- [x] Rust workspace и Tauri 2 shell;
- [x] Svelte + TypeScript diagnostic shell;
- [x] SQLite и migrations;
- [x] settings и structured logging;
- [x] provider ports и test infrastructure;
- [x] Windows/Linux CI skeleton.

## 2. Bulk ingestion

- [x] RelicsRunProvider с поиском `today..today-5`;
- [x] FrameForgeMirrorProvider;
- [x] отдельный catalog provider/cache;
- [x] bounded download, validation, checksum, atomic promotion;
- [x] SQLite import и diagnostics: provider/date/item count/record count;
- [x] offline parser/storage integration tests и opt-in production smoke-test.

## 3. Pricing Engine

- [x] сначала fixtures/tests;
- [x] closed/sell/relic/rank/subtype/stars logic;
- [x] outlier clusters, thin market, confidence/freshness;
- [x] structured explanation и IPC application service.

## 4. Market Browser

- [x] быстрый локальный поиск по slug и локализованным именам;
- [x] bulk fair price, volume, confidence, freshness и абсолютная дата;
- [x] плотная сортируемая таблица с адаптивными карточками;
- [x] item detail и «Почему такая цена?» без обязательного network;
- [x] keyboard search, semantic table, live region и responsive QA до 320 px.

## 5. Live Pricing

- [x] WFM v2 `/top` client, limiter, retry/backoff и `Retry-After`;
- [x] TTL cache, in-flight coalescing и stale cache fallback;
- [x] exact rank/subtype/stars, lowest ask, отдельные quantity-weighted low3/low5 и top buy;
- [x] live list/quick price и расширение «Почему такая цена?»;
- [x] offline fixtures, production smoke и desktop/320 px UI QA.

## 6. History

- [x] incremental background import до 90 дней, не более семи новых дней за запуск;
- [x] compact daily aggregates без долговременного хранения raw JSON;
- [x] median/change/volume для 7/30/90 и minimum volume floors;
- [x] HOLD/NEUTRAL/SELL/PEAK с обязательным live-подтверждением PEAK;
- [x] доступный SVG chart и точечный IPC без загрузки всей истории во frontend.

## 7. Inventory

- [x] bounded PlatScope JSON v1 import и общий `InventoryProvider` contract;
- [x] resolver по canonical ID/game path/slug без join по display name;
- [x] conservative tradeability, exact rank/subtype и `Keep N` reserve;
- [x] coherent snapshot, атомарный LKG и локальный Inventory UI;
- [x] встроенный read-only scanner на основе MIT-кода TennoWorth;
- [x] real-game smoke: 663 072 байта, 1 808 нормализованных строк из allowlist-категорий TennoWorth, build `2026.08.19.11.06`;
- [x] bounded helper/Overwolf inventory.json import с conservative unknown tradeability;
- [x] legacy Overwolf/helper envelope сохранён только как совместимый import fallback, не как основной product path;
- [x] комбинируемые category/tradeability/vaulted/duplicates/priced фильтры на normalized LKG;
- [x] process access изолирован: Windows query/read only, credentials не пересекают WebView и не логируются;
- [x] automatic acquisition работает без Overwolf; риск стороннего memory read честно указан в UI.

## 8. Sell Now

- [x] inventory + bulk pricing + history;
- [x] bounded Sell Priority с confidence/timing и edge-case тестами;
- [x] поиск, сортировка, timing/confidence-фильтры и встроенные рабочие виды;
- [x] номинальная стоимость отдельно от реалистичной ликвидности;
- [x] explainability цены/priority и on-demand live Quick Sell точного варианта.

## 9. Sets, Relics, Ducats

- [x] normalized WFCD metadata и отдельный атомарный LKG;
- [x] set composition и точные recipe quantities;
- [x] set-vs-parts с confidence/liquidity-adjusted сравнением;
- [x] relic EV для Intact/Exceptional/Flawless/Radiant с partial/insufficient pricing;
- [x] ducat efficiency только по credible fair price;
- [x] vault status как дополнительный контекст без ценового прогноза.
- [x] mastery requirement из WFCD в exact Item Detail с явным состоянием отсутствующих данных.

## 10. WFM account

- [x] отдельный opt-in auth module, не связанный с market fetching;
- [x] OS secure storage без plaintext fallback;
- [x] чтение profile и my orders через WFM v2;
- [x] explicit create/update/delete/visibility actions;
- [x] двойное подтверждение writes на UI и service boundary;
- [x] никаких автоматических writes и account network до opt-in.

## 11. Диагностика

- [x] отдельный экран вместо компактного diagnostic shell;
- [x] provider health: attempt/success/error/latency/failure streak;
- [x] явные состояния OK/degraded/error/not checked без сетевой проверки при открытии;
- [x] локальное покрытие market/catalog/history/inventory и offline-ready state;
- [x] live WFM request обновляет health, TTL cache не изображает новый сетевой успех;
- [x] диагностический DTO не содержит credentials, account IDs, nonce и raw inventory.
- [x] явный атомарный экспорт versioned allowlist-report без локального DB path.

## 12. Packaging

- [x] Windows x64 NSIS и Linux x86_64 AppImage закреплены в Tauri config;
- [x] ручной matrix workflow собирает и сохраняет оба installer artifact;
- [x] workflow не создаёт GitHub Release и не требует write permission;
- [x] code signing policy, граница доверенного релиза и SHA-256 manifests;
- [ ] защищённые signing credentials, release environment и реально подписанные artifacts;
- [ ] auto-update после появления подписанного release channel.

## 13. Dashboard

- [x] отдельные nominal inventory value и sellable value без подмены неизвестных цен;
- [x] лучшие кандидаты используют существующий Sell Priority;
- [x] items worth checking объединяют inventory attention и отсутствующие цены;
- [x] явные диапазоны слабой ликвидности без обещания времени продажи;
- [x] абсолютная свежесть рынка, локальное покрытие истории и offline-ready state;
- [x] только локальная композиция без network side effects при открытии.

## 14. Локализация

- [x] русский и английский интерфейс с типизированным Svelte locale store;
- [x] отдельный экран настроек и сохранение `Language` без сброса остальных параметров;
- [x] перезагрузка локальных DTO после смены языка для локализованных display names;
- [x] locale-aware числа, даты, статусы, ошибки, empty states и доступные имена controls;
- [x] объяснения pricing по стабильным reason codes, а selling/insights — по типизированным фактам DTO;
- [x] canonical identity, variant keys и расчёты не зависят от языка.

## 15. Производительность

- [x] offline-first startup не ждёт provider network;
- [x] bulk refresh автоматически проверяется после старта и периодически по `bulk_refresh_hours` без двойных download;
- [x] WFCD metadata автоматически проверяется после появления каталога и не блокирует price lifecycle;
- [x] экран настроек управляет платформой, crossplay, bulk refresh interval и live quote TTL; PC-only bulk не используется как цена другой платформы, IPC-сохранение отклоняет значения вне operational bounds;
- [x] market search работает по локальному нормализованному индексу;
- [x] основной результат ограничен 60 строками, service API — 100 строками;
- [x] история загружает до 90 компактных точек только выбранного exact variant;
- [x] офлайн-тест на 4 000 вариантах проверяет bounded-результат и бюджет 250 мс;
- [x] виртуализация обязательна до будущего снятия bounded-лимита, но не усложняет текущую семантическую таблицу.

## 16. Riven

- [x] отдельный `MarketItemKind::Riven`, не смешанный со standard/relic pricing;
- [x] bulk/live medians не создают цену уникального roll, confidence остаётся Unknown;
- [x] отдельный локализуемый reason code и regression tests;
- [x] weapon disposition и общая статистика multiplier из поддерживаемого WFCD metadata snapshot;
- [x] searchable Riven UI с отдельным disclaimer и без цены уникального roll.

## 17. Рабочие представления

- [x] Market, Inventory и Sell Now восстанавливают последние фильтры и сортировку после перезапуска WebView;
- [x] каждый экран использует отдельный versioned storage key и allowlist-validation;
- [x] повреждённое, устаревшее или недоступное storage состояние не блокирует UI и сбрасывается к defaults;
- [x] свободные поисковые строки не сохраняются;
- [x] unit tests и browser reload QA подтверждают round-trip всех трёх экранов.

## MVP gate

MVP требует working Windows и Linux builds, offline LKG startup, primary/mirror fallback, schema drift protection, exact variants, market search, live refresh, fair/list/quick prices, confidence/freshness/explanation и pricing edge-case coverage.

Текущие прямые доказательства и непроверенные внешние gates перечислены в [аудите готовности](COMPLETION_AUDIT.md). Наличие workflow без успешного artifact не считается доказательством Linux build.
