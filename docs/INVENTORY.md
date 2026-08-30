# Инвентарь

## Статус

Этап 7 реализован полностью: встроенный read-only scanner либо ручной JSON import → validation → resolver → атомарный SQLite LKG → Inventory UI. Реальный smoke 2026-08-28 получил 665 443 байта с Warframe build `2026.08.19.11.06` и нормализовал 2 148 строк.

Read-only normalizer читает только 12 явно разрешённых верхнеуровневых inventory-категорий: `MiscItems`, `Recipes`, `RawUpgrades`, `Upgrades`, `Suits`, `LongGuns`, `Pistols`, `Melee`, `SpaceGuns`, `SpaceMelee`, `Sentinels`, `SentinelWeapons`. `RawUpgrades` содержит стеки модов и мистификаторов нулевого ранга, а `Upgrades` — отдельные прокачанные экземпляры; их точный ранг извлекается из поля `lvl` внутри `UpgradeFingerprint`. Отсутствующий `lvl` в корректном fingerprint означает ранг 0. Категории складываются как разные ранговые варианты, поэтому копии не задваиваются. Normalizer не обходит профиль рекурсивно, поэтому UI-настройки, темы и другие строки `/Lotus/Interface/...` не могут стать предметами. Торговый интерфейс показывает однозначно сопоставленные позиции; отдельный экран установленных модов дополнительно может показать защищённую копию без свободного остатка. Копии с `XP > 0`, которые DE пометила непродаваемыми, вычитаются из доступного для продажи количества по схеме TennoWorth; canonical path не используется как видимое имя.

Для учёта сборок scanner сопоставляет уникальные `Upgrades[].ItemId` со ссылками в `Configs[].Upgrades` у варфреймов, оружия, компаньонов, арчвингов, некрамехов и усилителей. Один физический экземпляр мода считается один раз, даже если он используется в нескольких конфигурациях A/B/C. Идентификаторы экземпляров перед сохранением заменяются SHA-256-ключами; raw ID аккаунта, предмета и мода не сохраняются.

## Порт

```text
InventoryProvider
  -> PlayerInventorySnapshot
     source
     observed_at
     game_build?
     account_fingerprint?   # не secret и не raw account ID
     items[]
     validation
```

UI и Pricing Engine не знают, пришёл snapshot из read-only scan, JSON import или fixture.

## Identity и variants

Inventory item сначала сохраняет canonical game path/ID и фактические измерения:

```text
canonical_game_id
quantity
rank?
subtype/refinement?
tradeability
leveled
source_context
```

Resolver отдельно сопоставляет его с market `slug`. Display name не является join key.

## Coherent snapshot

Snapshot принимается только целиком после проверок generation, bounds, уникальности, count consistency и допустимых identities. Валидный новый snapshot авторитетен и может уменьшать quantity. Partial, stale или inconsistent snapshot не изменяет LKG inventory.

## Read-only process adapter

Scanner перенесён из MIT-кода [TennoWorth](https://github.com/tennoworth/tennoworth) и изолирован в `platscope-readonly-scan`. На Windows он использует только `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ`, `VirtualQueryEx` и `ReadProcessMemory`; на Linux читает `/proc/<pid>/maps` и `/proc/<pid>/mem`. Запись в память, hooks и injection отсутствуют.

Из памяти выбирается доминирующая пара `accountId`/`nonce` и build/`ct`; затем выполняется один HTTPS-запрос к `https://api.warframe.com/api/inventory.php`. Credentials живут только внутри Rust scanner, не возвращаются в WebView, не сохраняются и не логируются. Raw memory и raw inventory JSON не записываются на диск. Снимок публикуется только после существующей bounded-нормализации и exact resolver.

Digital Extremes не публикует allowlist и не гарантирует отсутствие санкций для стороннего memory read, поэтому UI прямо сообщает о риске. Действие запускается пользователем явно.

## Import fallback

Рабочий адаптер автоматически различает PlatScope JSON v1, совместимый helper/Overwolf `inventory.json` и строгий companion envelope v1. Общие ограничения: payload до 8 МиБ, до 100 000 строк, `quantity` от 1 до 100 000 000 и rank до 100. Увеличенный quantity-bound подтверждён реальным инвентарём с количеством ресурса 1 552 763. Для внешнего export дополнительно ограничены nesting до 64 уровней и обход до 250 000 JSON nodes.

В настройках можно явно включить автоматическую проверку абсолютного пути к companion JSON. Poller работает в фоне раз в три секунды, ждёт два одинаковых metadata sample, не повторяет импорт неизменившегося файла и уведомляет открытый Inventory UI после успешной публикации. Автоматический путь принимает только `producer: platscope-overwolf-companion`; ручной import по-прежнему поддерживает PlatScope/helper formats.

```json
{
  "schemaVersion": 1,
  "observedAt": "2026-08-27T08:30:00Z",
  "itemCount": 1,
  "items": [{
    "canonicalGameId": "nyx_prime_set",
    "quantity": 2,
    "rank": null,
    "subtype": null,
    "tradeability": "tradeable",
    "leveled": false
  }]
}
```

Helper adapter читает только объекты с canonical `ItemType`, опциональными `ItemCount` и `Rank`/`UpgradeLevel`. Повторяющиеся exact rows группируются с checked arithmetic. Поддерживаются raw inventory response и Overwolf-style wrapper с `value` object/JSON string. Raw JSON после нормализации не сохраняется.

Внешний export не доказывает instance tradeability. Все его строки получают `Tradeability::Unknown`: они могут участвовать в owned/set/relic расчётах, но имеют `sellable = 0` и не появляются в торговом интерфейсе «Мои предметы». Это намеренная fail-closed граница, а не недостаток распознавания.

Из внешних инструментов перенесён только MIT-код scanner TennoWorth с обязательной атрибуцией. `warframe-api-helper` не используется из-за несовместимого Commons Clause.

## Tradeability и reserve

Состояния:

```text
Tradeable
Untradeable
Unknown
```

`Unknown` не попадает в «продавать сейчас». Для exact-группы текущего этапа применяется:

```text
protected = untradeable + equipped_tradeable
sellable = min(tradeable - equipped_tradeable, owned - max(Keep N, protected))
```

Если есть неизвестная tradeability, предмет не сопоставлен или точный rank/subtype отсутствует в текущем LKG, `sellable = 0`. Leveled quantity показывается отдельно и не объявляется untradeable без данных источника.

## Хранение и UI

Миграция 6 добавляет `inventory_snapshots` и `inventory_items`, а миграция 11 — признак полного сканирования сборок, количество надетых копий и `inventory_mod_placements`. Новый снимок становится текущим одной транзакцией; invalid/partial import не изменяет предыдущий LKG. Raw JSON после validation не сохраняется.

Экран «Мои предметы» показывает распознанные торговые строки, owned/sellable, источник и абсолютное время снимка. Предмет с известным catalog ID, но без точного rank/subtype, остаётся видимым со статусом проверки и `sellable = 0`; создать ордер для него нельзя. Unknown tradeability и неизвестные item ID не проходят границу торгового UI. Фильтры комбинируются независимо:

- category;
- весь инвентарь / к продаже / продавать сейчас / лучше подождать / требуют проверки;
- все / не надеты / надеты;
- duplicates;
- priced/unpriced;
- `Keep 0/1/2`.

Отдельный экран «Надетые моды» группирует моды по физическому экземпляру предмета и конфигурациям A/B/C. Экран только показывает место установки: снять мод по-прежнему нужно в Арсенале Warframe.

`VaultStatus` берётся только из текущего normalized WFCD metadata LKG: статус set наследуется его component-строками, relic и prime part используют собственные canonical slug. Отсутствующая или противоречивая metadata даёт `unknown`, а не предположение по имени.

`Priced` означает, что Pricing Engine вернул надёжный bulk `fair_price` для точного `MarketVariantKey`. Отсутствие цены остаётся `unpriced`; UI не показывает его как `0p`. Эти признаки вычисляются локально при чтении Inventory DTO и не запускают provider network. При смене платформы сохранённые структурные варианты проецируются на новый platform key при чтении; PC bulk больше не совпадает, поэтому строки становятся `unpriced`, но не исчезают и остаются доступны для явного live-запроса.

## Privacy

- данные остаются локально;
- telemetry и upload отсутствуют по умолчанию;
- diagnostic export редактирует paths, account IDs, nonce, tokens и usernames;
- raw memory dumps не сохраняются обычным режимом;
- debug capture возможен только отдельным явным действием с предупреждением и auto-redaction, где это возможно.
