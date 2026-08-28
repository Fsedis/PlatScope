# Pricing Engine

## Состояние реализации

Этап 3 реализован в `crates/platscope-pricing`. Движок является чистой функцией и не читает сеть или SQLite. `PricingService` в application layer загружает из текущего LKG только записи точного platform/rank/subtype/stars варианта и передаёт их движку. Цена PC никогда не используется как fallback для другого platform key.

Пороговые константы:

```text
MIN_TRUSTED_CLOSED_VOLUME = 3
THIN_MARKET_VOLUME = 5
FRESH_DAYS = 2
AGING_DAYS = 7
```

Golden fixtures находятся в `fixtures/pricing/golden_scenarios.json`. Они фиксируют normal liquid market, isolated troll ask, реальный кластерный обвал, thin fantasy ask, sell-only fallback и relic sell-only. Дополнительные unit tests проверяют exact rank/refinement, offline/mismatched live book, рост рынка, fallback provider и stale source.

IPC contracts `price_current_variant` и `live_price_current_variant` доступны desktop shell. Market Browser использует тот же движок через `MarketBrowserService`, а `LivePricingService` повторно рассчитывает рекомендацию с точным `LiveOrderBook`: frontend получает готовые числа и структурированные причины, не дублируя формулы.

## Цель

Pricing Engine превращает нормализованные market signals в объяснимую оценку. Он не знает источник JSON и не выполняет network/SQLite операции.

Вход:

```text
NormalizedMarketSnapshot
LiveOrderBook?
PriceHistory?
MarketVariantKey
```

Выход:

```text
fair_price?
list_price?
quick_sell?
lowest_ask?
depth_three?
depth_price? # low5
confidence
freshness
explanation[]
```

## Базовые правила

1. `closed` — сигнал состоявшихся сделок и основной baseline.
2. Trustworthy closed volume начинается с тестируемой константы `3`, но будет калиброваться на fixtures.
3. Для обычного item при trusted closed и sell median допустим консервативный baseline `min(closed_median, sell_median)`.
4. Для relic sell median не становится fair price: bulk listings и refinement distortions делают его слабым сигналом.
5. Точная platform/rank/subtype/stars комбинация обязательна. Соседний tier или PC-рынок не подставляются молча.
6. Нет сигнала — `None`, confidence `Unknown`.

## Live book

Перед расчётом исключаются:

- invisible orders;
- неподходящая side;
- неподходящий platform/crossplay context;
- offline users для обещания «сейчас»;
- несовпадающий rank/subtype/charges/stars;
- некорректные quantity/perTrade/price.

Рассчитываются `lowest_ask`, `low3`, `low5`, `top_buy`, количество orders и суммарная доступная depth quantity. `low3` и `low5` — отдельные quantity-weighted средние цены до первых трёх и пяти доступных sell units; если units меньше целевого окна, среднее строится только по фактически доступной глубине и не выдаётся за полный стакан.

## Credible lowest ask

Единичный минимум не равен цене. Начальная эвристика-кандидат:

```text
если lowest < fair / 3
и следующий кластер согласован около fair,
lowest помечается isolated outlier
```

Но кластер `[10, 11, 11, 12, 12]` при старом fair `40` означает вероятный сдвиг рынка. Решение принимает cluster-level функция, а не последовательное удаление всех «слишком низких» значений.

На thin market одиночный ask выше baseline не повышает fair price. List price ограничивается диапазоном baseline и получает Low confidence.

## Quick Sell

Quick Sell — лучший валидный live buy order для точного варианта с учётом доступной quantity. Bulk buy median без live book отображается как исторический buy signal, но не называется гарантированной немедленной продажей.

## Riven safety boundary

`MarketItemKind::Riven` не проходит обычный item pricing path. Даже при наличии bulk closed median или live orders результат содержит `fair/list/quick/lowest/depth = null`, `confidence = Unknown` и reason `riven_pricing_unsupported`. Это намеренный отказ от ложной точности уникального roll. Weapon disposition и общий multiplier читаются из отдельного WFCD metadata-модуля и показываются как контекст оружия; они не превращаются в «точную цену» конкретного roll.

## Confidence

Confidence считается отдельно от freshness:

- `High`: свежий bulk, достаточный closed volume, exact variant, live cluster согласован;
- `Medium`: trusted closed data, но мало live depth или snapshot стареет;
- `Low`: только asks, thin market, provider fallback или конфликт signals;
- `Unknown`: недостаточно данных или variant mismatch.

Каждое понижение confidence добавляет machine-readable snake_case reason и локализуемое объяснение.

## Freshness

Freshness использует `source_date`, а не время скачивания. Категории и пороги остаются настройками доменного сервиса; UI всегда показывает абсолютную дату/возраст.

## Sell Priority

Это ranking score, не обещание дохода. Количество учитывается через приблизительную absorption capacity рынка:

```text
sellable_quantity
credible clearing price
closed/live liquidity
confidence penalty
timing signal
```

Нельзя ранжировать `200 × price`, если рынок поглощает единицы. Nominal Value вычисляется отдельно и снабжается предупреждением.

## Объяснение

`PriceExplanation` хранит использованные signals и причины исключений. Пример:

```text
fair основан на 46 closed trades
live low5: 37–42p
ask 1p исключён как isolated outlier
source date: 2026-08-26
confidence: High
```

UI не реконструирует объяснение из числа — получает готовую структурированную модель.

## Обязательные тесты перед production code

- normal liquid market;
- troll ask `1p`;
- fantasy ask `3000p`;
- реальный кластерный обвал;
- thin market;
- no closed/sell/buy orders;
- exact rank и только max-rank data;
- relic intact/radiant;
- stale fallback snapshot;
- numeric strings и invalid non-finite values;
- disagreement bulk/live;
- quantity/perTrade depth.

Пороговые константы не живут в UI и проверяются boundary tests.
