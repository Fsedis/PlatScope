# Аккаунт Warframe Market

## Статус этапа

Этап 10 реализует отдельную opt-in интеграцию аккаунта WFM: вход, чтение собственных ордеров и явно подтверждённые create/update/delete/visibility операции. Модуль не участвует в bulk/live pricing и не нужен для основной работы PlatScope.

Актуальный контракт проверен по официальной документации WFM API `v0.25.0`:

- [обзор HTTP API](https://docs.warframe.market/docs/api/overview/);
- [orders](https://docs.warframe.market/docs/api/orders/);
- [модели данных](https://docs.warframe.market/docs/data-models/);
- [OAuth](https://docs.warframe.market/docs/oauth/overview/).

## Почему вход переходный

Публичная регистрация OAuth 2.0 clients пока недоступна сторонним приложениям. PlatScope не выдаёт себя за first-party client и не обходит Firebase App Check. До появления поддерживаемой регистрации используется официальный переходный путь, описанный WFM:

```text
одноразовый legacy v1 sign-in
  -> JWT из response header
  -> только JWT в OS secure storage
  -> /v2/me и /v2/orders/my
  -> явные /v2/order writes
```

Пароль существует только в памяти на время одного sign-in request. Он не записывается в SQLite, settings, файлы или логи. После завершения frontend очищает поле пароля. Если WFM отклоняет проверку `/v2/me`, только что сохранённый токен удаляется.

## Secure storage

`OsCredentialStore` использует crate `keyring`:

- Windows — нативное Windows credential storage;
- Linux — системный Secret Service backend при его наличии;
- service: `PlatScope.WarframeMarket`;
- account: `default-session`.

Plaintext fallback запрещён. Ошибка keychain выключает account-функцию, но не мешает market, pricing, inventory и analytics. В SQLite сохраняется только стабильный несекретный device ID; token, email и password туда не попадают.

`AccountToken` обёрнут в `Zeroizing<String>`, не реализует serialization и всегда выводится как `AccountToken([REDACTED])` через `Debug`.

## API boundary

Отдельный crate `platscope-account` владеет transport DTO, secure storage и bounded HTTP client. Основные ограничения:

- отдельные v1/v2 base URLs;
- HTTPS production endpoints;
- connect timeout 5 секунд, request timeout 15 секунд;
- не более одного account request одновременно;
- интервал 350 ms, то есть ниже официального лимита 3 requests/second;
- response body не более 2 MiB независимо от `Content-Length`;
- локальная проверка documented bounds до write;
- `Authorization` и token не включаются в ошибки и tracing fields.

Поддержанные v2 routes:

| Действие | Route | Изменяет WFM |
| --- | --- | --- |
| Профиль | `GET /v2/me` | нет |
| Мои ордера | `GET /v2/orders/my` | нет |
| Создать | `POST /v2/order` | да |
| Изменить/visibility | `PATCH /v2/order/{id}` | да |
| Удалить | `DELETE /v2/order/{id}` | да |
| Завершить сессию | `POST /v2/auth/signout` | да, затем локальный token удаляется всегда |

JSON следует текущей схеме: поле типа ордера называется `type`; variant fields `rank`, `charges`, `subtype`, `amberStars`, `cyanStars` и `perTrade` пропускаются, когда не применимы, а не отправляются как `null`.

## Явное подтверждение writes

Защита дублируется на двух границах:

1. UI сначала создаёт локальный черновик и показывает финальную сводку;
2. пользователь отмечает «Я проверил действие и параметры ордера»;
3. только затем frontend вызывает Tauri command с `confirmed: true`;
4. `AccountService` повторно отклоняет любой create/update/delete без confirmation.

Нет таймеров, background sync или автоматической публикации. На отдельном экране аккаунта первоначальный `visible` для нового ордера выключен. В карточке кандидата Sell Now видимость задаётся явным переключателем и входит в сводку подтверждения. Удаление отдельно сообщает, что восстановление потребует создать ордер заново.

Карточка кандидата Sell Now получает WFM `itemId` через локальный catalog join и ищет собственный sell-ордер по полному варианту: `itemId`, rank, subtype, `amberStars` и `cyanStars`. Это не позволяет показать или изменить ордер другого ранга как ордер выбранного предмета.

Признак `bulkTradable` сохраняется из каталога WFM. Для bulk-предметов create/update передаёт обязательный `perTrade`; интерфейс валидирует диапазон 1–6 и делимость общего количества до отправки. Для обновления со старого catalog schema поддерживается безопасное распознавание категорий WFM, которые целиком состоят из bulk-предметов: реликвии, мистификаторы, рыба, самоцветы, скульптуры и звёзды Ayatan.

## Тестирование

Offline coverage включает:

- redaction token в `Debug`;
- documented bounds для price/quantity/per-trade;
- отказ от пустого PATCH;
- точные v2 JSON field names и отсутствие `null` optional fields;
- десериализацию `/v2/me` и `/v2/orders/my` fixtures;
- обязательный backend confirmation;
- frontend draft validation и сохранение exact variant;
- browser mock flows для connect, create, update, delete и отказа без checkbox.

Live authenticated smoke-test намеренно не выполняется без учётных данных владельца. Production release проверяет компиляцию, startup и отсутствие обращения к account endpoints до явного opt-in.

## Известные ограничения

- OAuth 2.0 заменит legacy sign-in после появления публичной регистрации clients; миграция должна сохранить изоляцию account module.
- UI создания использует предмет из локального каталога. Для bulk, charge и иных условных полей WFM может потребоваться дополнительное значение; server validation возвращается пользователю без автоматического исправления.
- В compact order model WFM возвращает `itemId`, но не локализованное имя. В карточке Sell Now имя берётся из локального каталога; отдельный список ордеров аккаунта по-прежнему может показывать устойчивый WFM ID.
- Диагностический экспорт реализован отдельным allowlist DTO и не включает account IDs, usernames и credentials.
