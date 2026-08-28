# Исследование перед реализацией

Дата проверки: **27 августа 2026 года**. Рабочий каталог на момент начала был пуст и не являлся Git-репозиторием. Референсы изучались как источники edge cases и проверяемых идей; их код в PlatScope не копировался.

## Проверенные референсы

| Проект | Проверенный commit | Лицензия | Полезные выводы |
|---|---|---|---|
| [tennoworth/tennoworth](https://github.com/tennoworth/tennoworth) | `815a8db6802b959cacd4fe74ceb7d78fcfdc4e2e` | MIT | Раздельные tier/subtype, tri-state rank в старой WFM statistics schema, ограничение fantasy ask на тонком рынке, sell priority с ограничением рыночным поглощением, golden/parity fixtures. |
| [Deftera186/tennoscope](https://github.com/Deftera186/tennoscope) | `245f2b913e672ddffedd744f011e420420f3169c` | GPL-3.0 | Acquisition скрывается за интерфейсом; частичный inventory snapshot недопустим; coherent snapshot заменяется транзакционно; exact row identity включает rank/refinement; неизвестный вариант нельзя выставлять. |
| [WyrmStudios/FrameForge](https://github.com/WyrmStudios/FrameForge) | `5ab806f5113e8a01b080a51be5b3b725f734bad1` | GPL-3.0 | Read-only memory scan — отдельный рискованный модуль; stale/headless memory blobs встречаются реально; ranked и unranked copies приходят разными формами; кэш адреса требует инвалидирования при смене PID. |
| [WyrmStudios/FrameForgePricing](https://github.com/WyrmStudios/FrameForgePricing) | `10271745a74c804a0abac3f240f540a431dafa87` | лицензия отсутствует | Зеркало действительно публикует совместимый price dump, но на момент проверки содержит только `price_history_latest.json`, без обещанного `items.json`. |
| [finneritter/Warframe-Item-Tracker](https://github.com/finneritter/Warframe-Item-Tracker) | `1b2bce36e50427870c6957c19764316e8820339d` | proprietary, all rights reserved | Только наблюдение, без заимствования кода: migrations, отдельные ranks/orders/ohlc, platform adapters для read-only scan, явное согласие пользователя, fingerprint текущей версии игры. |
| [WFHelper/WFHelper](https://github.com/WFHelper/WFHelper) | `2ac4c91140b2da97eefa8a7bddf06a3bc33bb351` | MIT | Rank/subtype входят в cache key; negative cache отделяется от успешного; ranked lookup обязан fail closed при недоступном каталоге; inventory normalization сталкивается с несколькими именами полей, вложенным и double-encoded JSON. |

### Лицензионное решение

GPL-3.0 и proprietary проекты используются только для исследования поведения и списка тестов. Код из них не переносится. MIT-код также не копируется на foundation-этапе: более безопасно реализовать собственные малые контракты и сохранить чистое происхождение кода. Данные зеркала без лицензии используются только как внешний runtime-провайдер; его содержимое не встраивается в репозиторий.

## Реальный dump relics.run

Проверены URL:

- `https://www.relics.run/history/price_history_2026-08-27.json` → `404`;
- `https://www.relics.run/history/price_history_2026-08-26.json` → `200`, первый доступный валидный день;
- `https://www.relics.run/history/item_data/items.json` → `200`.

Это подтверждает обязательность поиска назад по UTC-дням, а не предположение о наличии сегодняшнего файла.

Снимок за 2026-08-26:

- размер: `3 878 900` байт;
- SHA-256: `62f5853e9faa0bfde72e6ef8027e4f3dc00e50c8a6b94c76f318079a8bfd510b`;
- 3 840 item buckets;
- 13 222 market records;
- `closed`: 3 200, `buy`: 4 163, `sell`: 5 859;
- 6 502 записи содержат `mod_rank`;
- 2 809 записей содержат `subtype`.

Корень price dump — объект `display name -> MarketRecord[]`, то есть display name является транспортным ключом, но не может быть внутренним ID. Record содержит подмножество полей:

```text
datetime, volume, min_price, max_price, open_price, closed_price,
avg_price, wa_price, median, moving_avg, donch_top, donch_bot,
id, item_id, order_type, mod_rank?, subtype?, amber_stars?, cyan_stars?
```

Каталог содержит 3 840 объектов. Наблюдавшиеся поля:

```text
id, slug, gameRef, tags, i18n,
maxRank?, subtypes?, bulkTradable?, ducats?, vaulted?,
baseEndo?, endoMultiplier?, maxAmberStars?, maxCyanStars?
```

1 517 предметов имеют `maxRank`, 857 — `subtypes`. В полученном dump `i18n` содержит только `en`; локализацию нельзя считать гарантированной частью этого источника.

## Реальное состояние зеркала

FrameForgePricing snapshot на момент клонирования:

- source date: 2026-08-25;
- размер: `3 845 375` байт;
- 3 838 item buckets;
- 13 120 records;
- набор полей совместим с relics.run;
- каталога `items.json` в репозитории нет.

Следствие: price snapshot и item catalog получают независимые metadata, validation и last-known-good lifecycle. Отсутствие свежего каталога не разрешает заменить валидный каталог пустым; price import допускается только если каждый record можно сопоставить по `item_id` с текущим совместимым каталогом либо безопасно поместить в quarantine.

## Актуальный Warframe.Market API

Проверена [официальная документация](https://docs.warframe.market/) версии API `0.25.0` и реальные ответы:

- production base: `https://api.warframe.market/v2/`;
- `GET /v2/items`: envelope `{ apiVersion, data: Item[], error }`, 3 840 items;
- `GET /v2/orders/item/{slug}`: возвращает visible orders, включая offline пользователей; endpoint сам по себе не является online order book;
- `GET /v2/orders/item/{slug}/top`: до пяти online buy и sell orders, поддерживает exact `rank`, `subtype`, charges и Ayatan stars;
- [общий лимит](https://docs.warframe.market/docs/rules/overview/): 3 запроса/с, возможны `429` и `509`, лимиты могут меняться;
- обязательны идентифицируемый `User-Agent`, кэширование и отсутствие tight polling.

Order v2 содержит:

```text
id, type, platinum, quantity, perTrade?, subtype?, rank?, charges?,
amberStars?, cyanStars?, visible, createdAt, updatedAt, itemId?, groupId?, user?
```

`GET /v1/items/{slug}/statistics` на дату проверки всё ещё отвечает и имеет legacy envelope `payload.statistics_closed/statistics_live` с окнами `48hours` и `90days`. Однако endpoint отсутствует в актуальной v2-документации. Он считается наблюдаемым legacy-источником, а не стабильным контрактом; bulk ingestion не должен от него зависеть.

## Найденные edge cases

1. `mod_rank` может отсутствовать, быть `null`, `0` или другим числом — отсутствие измерения не равно rank 0.
2. Реликвии и некоторые другие items имеют subtype; `intact` и `radiant` реально дают разные ряды.
3. WFM item orders содержат большое число offline/stale объявлений. Для live цены нужно фильтровать status или использовать `/top`.
4. `perTrade` и `quantity` различают размер объявления и размер одной сделки; цена остаётся ценой за trade unit согласно контракту конкретного item.
5. У тонкого рынка один fantasy ask нельзя превращать в portfolio value.
6. Старый memory blob может выглядеть валидным; нужен generation/freshness marker и атомарное принятие coherent snapshot.
7. Удаление предмета из нового coherent inventory snapshot является реальным уменьшением, но partial/invalid snapshot не должен обнулять прошлое состояние.
8. Catalog identity и inventory path принадлежат разным namespace; mapping должен быть явным и диагностируемым.
9. Неизвестная rank/subtype комбинация должна давать `unknown`, а не соседнюю цену.
10. Schema drift нельзя маскировать coercion в нули; допустимые строковые числа можно нормализовать только с диагностическим счётчиком.

## Решения до реализации

- Внутренний ключ: `slug + platform + rank? + subtype?`, с возможностью позже добавить charges/stars без миграции смысла существующих ключей.
- Catalog и price dump — разные агрегаты и разные LKG caches.
- Сырые записи сохраняются достаточно долго для диагностики, а доменная логика получает только нормализованные структуры.
- Live WFM используется точечно и кэшируется; bulk-рынок не строится тысячами запросов.
- Pricing Engine начинается с fixtures и unit tests; на foundation-этапе реализуются только типы и границы.
- Inventory acquisition откладывается до стабильного Pricing Engine; сейчас фиксируется порт и модель доверия.
