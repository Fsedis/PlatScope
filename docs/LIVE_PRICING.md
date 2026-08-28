# Live Pricing

## Источник и границы

PlatScope использует публичный `GET /v2/orders/item/{slug}/top` Warframe.Market. По официальному контракту endpoint возвращает до пяти лучших sell и пяти лучших buy orders от online-пользователей. Запрос передаёт `Platform`, `Crossplay`, `Language` и query-параметры точного rank/subtype/stars варианта.

Это read-only интеграция без авторизации. Приложение не создаёт, не изменяет и не удаляет объявления.

## Защита внешнего сервиса

- идентифицируемый `User-Agent`;
- не более одного запроса каждые 350 мс, что остаётся ниже общего лимита 3 req/s;
- connect timeout 8 секунд, общий timeout 20 секунд;
- максимум три попытки для `429`, `509` и `5xx`;
- `Retry-After` имеет приоритет, иначе exponential backoff с небольшим jitter;
- ответ ограничен 1 MiB и обязан быть JSON;
- UI не делает запрос на каждую строку или каждый поисковый ввод.

## Exact filtering

Сервер получает exact query-параметры, но provider повторно проверяет каждый order. Отбрасываются невидимые, нулевые, неизвестной стороны и несовпадающие rank, subtype, `amberStars` или `cyanStars`. Offline status сохраняется в provider-neutral модели, а Pricing Engine не использует его для обещания «сейчас».

## Cache и отказоустойчивость

Ключ cache состоит из `MarketVariantKey` и crossplay context. TTL берётся из settings, по умолчанию 90 секунд и ограничивается диапазоном 15–600 секунд. Один async mutex объединяет конкурентные запросы: после ожидания второй caller видит уже заполненный cache.

Если refresh завершился ошибкой, но старый quote существует, UI получает `stale_cache` и предупреждение. Без cache ошибка live не уничтожает bulk-рекомендацию: detail продолжает показывать offline fair price.

## Результат

`live_price_current_variant` возвращает:

```text
recommendation:
  fairPrice?
  listPrice?
  lowestAsk?
  depthThree? # quantity-weighted average до 3 sell units
  depthPrice? # quantity-weighted average до 5 sell units
  quickSell?
  confidence
  reasons[]
fetchedAt
quoteState: network | cache | stale_cache
sellOrderCount
buyOrderCount
orders[]:
  side
  platinum
  quantity
  perTrade
  userStatus
warning?
```

Counts и `orders[]` содержат только активных `online`/`in_game` пользователей. Для интерфейса книга повторно сортируется и ограничивается пятью минимальными sell и пятью максимальными buy; offline и невалидные rows не показываются и не участвуют в Quick Sell. Item Detail отдельно подписывает среднюю глубину до 3 и до 5 sell units, поэтому одиночный minimum не смешивается с более широким кластером. Исторический bulk buy signal по-прежнему не выдаётся за немедленную продажу.

## Проверка

Offline fixtures фиксируют envelope API `0.25.0`, exact rank filtering, quantity-weighted low3/low5, преобразование sell/buy и bounded active-order projection. Production smoke проверяет наличие обоих depth signals для `secura_dual_cestra`; повторный вызов возвращается из TTL cache.

Официальные источники: [API overview](https://docs.warframe.market/docs/api/overview), [Orders](https://docs.warframe.market/docs/api/orders), [Data Models](https://docs.warframe.market/docs/data-models), [Rules](https://docs.warframe.market/docs/rules/overview).
