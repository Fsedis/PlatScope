# Источники данных

## Общая политика

Каждый источник имеет отдельный transport DTO, parser, validation profile, health record и cache namespace. Данные разных bulk providers не смешиваются в одном snapshot.

## relics.run — primary bulk price provider

URL-шаблон:

```text
https://www.relics.run/history/price_history_YYYY-MM-DD.json
```

Поиск выполняется от текущей UTC-даты до `today - 5 days`. `404` для сегодняшнего дня является ожидаемым состоянием публикации, а не немедленной поломкой provider.

## Warframe Market — основной каталог предметов

Catalog URL:

```text
GET https://api.warframe.market/v2/items
Language: ru
Platform: pc
```

Каталог WFM v2 хранит canonical `slug` как идентичность, английское и русское имена, а также отдельные `thumb` для каждой локали. При русском языке приложение показывает русские названия во всех рыночных представлениях и русскую карточку мода. Для Prime-компонентов точное изображение детали из WFCD имеет приоритет над общей картинкой WFM. Локализация влияет только на отображение и поиск и никогда не заменяет ключ варианта.

Catalog schema v3 принудительно обновляет старый LKG без русских полей при следующей фоновой проверке. Уже сохранённый снимок инвентаря обогащается актуальным каталогом при чтении, поэтому повторный импорт не требуется.

relics.run остаётся основным источником текущих и исторических bulk-цен; его `item_id` сопоставляется с WFM catalog `id` до публикации снимка.

Price root на 2026-08-26:

```text
Map<display_name, RawMarketRecord[]>
```

`display_name` используется только для диагностики. Нормализация связывает запись по `item_id` с catalog `id`, получает `slug`, а затем строит `MarketVariantKey`.

Наблюдаемый daily dump не содержит отдельного платформенного измерения и нормализуется как `Platform::Pc`. Это явная граница покрытия: bulk-цены и bulk-history не переносятся на PlayStation, Xbox, Switch или Mobile.

Обязательные record fields:

```text
datetime: RFC3339 string
volume: finite non-negative number
median: finite non-negative number
order_type: closed | buy | sell
item_id: string
```

Условные измерения:

```text
mod_rank?: integer
subtype?: string
amber_stars?: integer
cyan_stars?: integer
```

Дополнительные числовые поля сохраняются для диагностики и последующего пересчёта. Не все поля присутствуют у каждого `order_type`.

## FrameForgePricing — mirror bulk price provider

Текущий raw URL:

```text
https://raw.githubusercontent.com/WyrmStudios/FrameForgePricing/main/price_history_latest.json
```

На 27 августа 2026 года зеркало не публикует `items.json`. Поэтому:

- mirror отвечает только за price dump;
- catalog берётся из собственного LKG cache или обновляется отдельным provider;
- несовместимость `item_id` с текущим каталогом отклоняет snapshot либо помещает неизвестные записи в quarantine по заранее заданному порогу;
- `provider = frameforge_mirror` фиксируется для всего snapshot.

## Local Cache Provider

Local cache возвращает только полностью promoted snapshot/catalog. Temporary download никогда не виден читателям. Metadata:

```text
provider
source_date
fetched_at
promoted_at
schema_version
item_count
record_count
checksum_sha256
```

Desktop lifecycle не блокирует старт сетью. После инициализации UI отдельный scheduler сразу проверяет возраст текущего LKG, затем повторяет проверку каждые пять минут. Реальный download выполняется только когда `promoted_at` старше `bulk_refresh_hours` (значение ограничивается диапазоном 1–24 часа) либо snapshot отсутствует. Ручной и фоновый refresh используют один async lock, поэтому не создают последовательный двойной download. UI получает `market-data-updated` только после успешной validation и atomic promotion; при provider failure остаётся прежний LKG.

Экран «Настройки» позволяет выбрать рыночную платформу, интервал bulk-проверки от 1 до 24 часов, TTL локального live quote от 30 до 600 секунд и crossplay-контекст явных WFM-запросов. Интерфейс прямо сообщает, что daily bulk покрывает только PC. Команда сохранения независимо проверяет полный объект: bulk interval 1–24 часа, live TTL 15–600 секунд и резерв inventory copies 0–10. Поэтому значения за пределами безопасного operational envelope не попадут в SQLite даже при прямом IPC-вызове.

## WFCD game metadata

Поддерживаемый источник игровых определений — [WFCD/warframe-items](https://github.com/WFCD/warframe-items). Он обновляется отдельно от bulk prices и атомарно публикуется как metadata LKG. Schema v2 добавила Riven `disposition` и точный `omegaAttenuation` из `Primary`, `Secondary`, `Melee`, `SentinelWeapons`, `Arch-Gun` и `Arch-Melee`. Schema v3 нормализует `masteryReq` в диапазоне 0–50 для предметов, точно сопоставленных WFM-каталогу; Item Detail различает настоящее `MR 0` и отсутствие определения.

Riven-поля используются только как общая характеристика оружия. Они не подаются в ordinary pricing path и не создают fair/list/quick price для уникального roll.

## Historical daily dumps

History bootstrap использует тот же immutable URL `price_history_YYYY-MM-DD.json`, transport limit, parser и validation profile, что и latest ingestion. После нормализации raw body освобождается; в SQLite остаются только `closed_median`, `closed_volume`, `sell_median` и `buy_median` точного варианта.

Текущий snapshot попадает в историю автоматически. При каждом запуске background service ищет пропуски назад до 90 дней и импортирует максимум семь новых дат. Такой incremental режим даёт минимальные 7 дней быстро и не создаёт burst из десятков больших запросов.

## Локальный inventory import

PlatScope JSON v1 — текущий поддерживаемый acquisition adapter. Файл выбирает пользователь; payload ограничен 8 МиБ, валидируется целиком и после нормализации не хранится. В SQLite попадают только checksum/metadata и resolved quantities. Инвентарь не отправляется внешним providers и не логируется.

Экспорты helper tools подключаются только отдельными versioned adapters после проверки схемы и лицензии. Строгий [Overwolf companion envelope v1](OVERWOLF_COMPANION.md) принимается отдельным opt-in poller: путь задаётся пользователем, стабильность файла проверяется до импорта, а helper JSON автоматически не принимается. Companion distribution и реальный GEP runtime остаются выключены до Overwolf approval. Read-only process scan не включается без актуальной policy/security проверки и явного opt-in.

## EE.log — подтверждение сделок

PlatScope читает только новые данные локального `%LOCALAPPDATA%\Warframe\EE.log` и не изменяет файл. Начало торгового диалога, блоки `You are offering` / `will receive` и success-маркер игры образуют один bounded event. Незавершённый диалог удаляется через 120 секунд; строка успеха без сохранённого диалога игнорируется.

Источник сообщает английские display names и количество, но не гарантирует точные измерения WFM-варианта. Поэтому журнал может предложить изменение только единственного нерангового ордера с точным английским именем. Он не заменяет inventory source и не применяется как источник цены.

## Warframe.Market — live provider

Официальная документация: [docs.warframe.market](https://docs.warframe.market/).

Используемые v2 endpoints:

```text
GET /v2/versions
GET /v2/items
GET /v2/orders/item/{slug}
GET /v2/orders/item/{slug}/top
```

Контекстные headers:

```text
User-Agent: PlatScope/<version> (+repository-or-contact)
Language: en | ru
Platform: pc | ps4 | xbox | switch | mobile
Crossplay: true | false
```

`Platform` берётся из сохранённого `AppSettings` и также входит в ключ live-cache. Смена платформы не может вернуть quote или bulk-рекомендацию от прежнего рынка.

Response envelope:

```text
{
  apiVersion: string,
  data: object | array | null,
  error: object | null
}
```

Для `/v2/orders/item/{slug}` offline orders не удаляются сервером. PlatScope фильтрует `visible`, нужную сторону, exact variant и acceptable online status. Для краткой live-котировки предпочтителен `/top`, но полный endpoint нужен для depth и объяснения.

Этап 5 использует `/v2/orders/item/{slug}/top`: endpoint возвращает до пяти sell и пяти buy orders от online-пользователей и принимает exact `rank`, `subtype`, `amberStars`, `cyanStars`. Клиент повторно проверяет эти измерения после парсинга. Контракт сверялся с официальной документацией API `v0.25.0` 27 августа 2026 года.

Текущий официальный общий предел — 3 запроса/с. Это конфигурационная константа с ссылкой на [правила](https://docs.warframe.market/docs/rules/overview/), а не вечное предположение. Реакция на `429`/`509`: respect `Retry-After`, exponential backoff с jitter, ограниченное число попыток, stale cache fallback.

Legacy `/v1/items/{slug}/statistics` наблюдался рабочим, но не входит в актуальную v2-документацию. Он не является foundation dependency.

## Validation gates

До promotion проверяются:

- HTTP success и JSON content type;
- body не больше 32 MiB;
- root shape;
- source date в допустимом диапазоне;
- item/record counts выше абсолютного floor;
- падение count относительно LKG не превышает заданный процент без ручного schema override;
- обязательные поля присутствуют;
- все используемые числа finite и неотрицательны;
- доля unknown item IDs ниже порога;
- rank/subtype совместимы с catalog;
- checksum вычислен до записи metadata.

Первоначальные floors задаются конфигурацией и тестами после нескольких наблюдений; значения текущего dump не хардкодятся как вечная схема.

## Health и кэш

Для каждого provider хранятся:

```text
last_attempt
last_success
last_error_code
last_error_message_redacted
latency_ms
consecutive_failures
```

Режимы:

- bulk update check — при старте и раз в несколько часов;
- immutable historical day — не скачивать повторно после успешного импорта;
- live quote TTL — 90 секунд по умолчанию;
- negative live cache — короче успешного и в отдельном key namespace;
- catalog refresh — по `/v2/versions` hash или умеренному TTL.
