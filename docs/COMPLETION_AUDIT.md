# Аудит готовности по master-guideline

Дата проверки: 28 августа 2026 года. Источник требований: приложенный `PlatScope: полный master-guideline для разработки`. Статус `Проверено` означает наличие прямого доказательства в текущем workspace; workflow-конфигурация сама по себе не считается успешным прогоном.

## MVP Definition of Done

| Требование | Статус | Доказательство / пробел |
| --- | --- | --- |
| Windows build работает | Проверено | `cargo tauri build --bundles nsis`; канонический `platscope.exe` прошёл startup smoke, NSIS и SHA-256 созданы. |
| Linux build работает | Не проверено | Tauri `appimage` target и Ubuntu workflow настроены, но в текущем окружении нет установленного WSL-дистрибутива, Docker или Podman, а workspace не подключён к GitHub repository; фактического AppImage artifact нет. |
| Offline startup с cache | Проверено | startup не ждёт provider network; release process остаётся живым и отвечает; локальные commands читают SQLite LKG; bulk refresh запускается отдельным due-aware scheduler и публикует UI event только после atomic promotion. |
| relics.run primary | Проверено | production `RelicsRunProvider`, parser fixtures и integration tests. |
| FrameForgePricing mirror fallback | Проверено | отдельный provider и fallback orchestration/tests. |
| Повреждённый upstream не ломает cache | Проверено | validation + atomic promotion и тесты сохранения предыдущего price/catalog/metadata/inventory LKG. |
| Нормализация market dump | Проверено | normalized domain model, bounded parser и fixtures. |
| Rank/subtype/refinement разделены | Проверено | входят в exact `MarketVariantKey`; boundary tests не подменяют варианты. |
| Market search и item detail | Проверено | локальный bounded search, detail DTO и browser QA. |
| Live WFM refresh | Проверено | exact top-orders client, limiter/cache/coalescing, active-only bounded current-order DTO, отдельные quantity-weighted low3/low5 и offline fixtures. |
| Настройки сетевого lifecycle | Проверено | UI управляет платформой, crossplay, bulk interval и live TTL нативными controls; Tauri повторно валидирует границы до записи в SQLite. Для не-PC ключей PC-only bulk/history не используются, а live WFM получает выбранную платформу. |
| Fair/List/Quick Sell | Проверено | отдельные nullable outputs и edge-case tests. |
| Confidence/Freshness/Explanation | Проверено | typed fields, stable snake_case reason codes, RU/EN presentation. |
| Pricing edge cases | Проверено | unit и golden fixtures: troll/fantasy asks, thin market, clusters, missing data, exact variants, relics, stale/fallback. |
| TennoWorth read-only scanner | Проверено | MIT-модуль перенесён с атрибуцией; реальный Windows smoke получил DE inventory JSON и 1 808 нормализованных строк из 11 inventory-категорий TennoWorth на build `2026.08.19.11.06`. |

## Первый usable release

| Требование | Статус | Доказательство / пробел |
| --- | --- | --- |
| Автоматически читается inventory | Проверено | Встроенный read-only scanner TennoWorth на реальном запущенном Warframe получил 663 072 байта и передал 1 808 строк в bounded parser; технические разделы профиля не обходятся, а UI показывает только разрешённые в торговом каталоге позиции с tradeable-копиями. Непродаваемые копии вычитаются до UI. Tauri command публикует результат через существующий atomic LKG pipeline. |
| Sellable copies | Проверено | exact resolver, tradeability и Keep N; unknown всегда даёт `sellable = 0`. |
| Sell Now / Sell Priority | Проверено | inventory + pricing + history, filters/sorting/explanations/live Quick Sell и tests. |
| History/trend | Проверено | bounded background import, 7/30/90, chart и timing tests. |
| Nominal portfolio value | Проверено | отделён от sellable value и не использует отсутствующую цену. |
| Видна неликвидность | Проверено | confidence/liquidity warnings и Dashboard attention. |
| Set/relic/ducat value | Проверено | normalized metadata LKG, due-aware background refresh, set-vs-parts, partial relic EV и credible plat/ducat. |
| Mastery requirement | Проверено | WFCD schema v3, строгий диапазон 0–50, атомарная SQLite-проекция и exact-slug Item Detail; `MR 0` не смешивается с отсутствующими данными. |
| Diagnostics | Проверено | provider health, local coverage, redacted export и отсутствие network-on-open. |
| Saved working views | Проверено | Market, Inventory и Sell Now сохраняют только allowlisted filters/sort в отдельных versioned keys; unit tests и browser reload QA подтверждают восстановление. |

## Дополнительные safety requirements

| Требование | Статус | Доказательство / пробел |
| --- | --- | --- |
| Уникальный Riven не оценивается обычной median | Проверено | отдельный `MarketItemKind::Riven`; standard bulk/live prices подавляются, confidence `Unknown`, отдельный reason code и tests. |
| Riven disposition/general statistics | Проверено | WFCD metadata snapshot нормализует `disposition`/`omegaAttenuation` для шести weapon-категорий; searchable UI показывает count/average/range и явно не оценивает уникальный roll. |
| Read-only process access изолирован | Проверено | Отдельный crate — единственная unsafe FFI boundary; Windows запрашивает только query/read, credentials не пересекают Tauri/WebView и не логируются. |
| Windows/Linux packaging | Частично | Windows NSIS проверен; Linux AppImage только сконфигурирован, фактическая Linux-сборка не доказана. |
| Code signing | Внешний gate | Policy и fail-closed artifact preflight готовы; QA mode подтверждает checksums и явно показывает `NotSigned`, trusted mode требует `Valid` signer/timestamp и Linux detached signature. Нет сертификата, защищённых credentials и signed artifact. |
| Auto-update | Внешний gate | Намеренно выключен до signed release channel. |

## Что доказывает завершение оставшихся пунктов

1. **Linux:** успешный Ubuntu job, сохранённый AppImage + `SHA256SUMS.txt`, запуск AppImage в чистой поддерживаемой среде.
2. **Signing:** защищённый release environment, `Valid` Authenticode, утверждённая Linux detached signature и повторная проверка опубликованных artifacts.
3. **Updater:** подписанный manifest/artifact, negative signature tests, recovery/rollback и ручной installer fallback.

До появления этих доказательств master-goal не считается полностью завершённым.
