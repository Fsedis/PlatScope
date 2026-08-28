# Overwolf companion

## Решение

Automatic inventory acquisition не встраивается в Tauri-процесс PlatScope. Разрешённая архитектура — отдельное opt-in приложение Overwolf, которое получает поддерживаемый Warframe GEP event и пишет локальный versioned export. PlatScope принимает этот файл через тот же bounded/fail-closed inventory boundary.

На 27 августа 2026 года официальный [Warframe GEP](https://dev.overwolf.com/ow-native/live-game-data-gep/supported-games/warframe) указывает game ID `8954` и info update `match_info.inventory`. Доступность события может временно меняться, поэтому companion обязан показывать состояние GEP и не выдавать старый снимок за новый.

Использование Overwolf API требует approval/whitelist app idea по [правилам compliance](https://dev.overwolf.com/ow-electron/guides/game-compliance/overview/). Пока approval не получен, PlatScope не распространяет OPK и не называет automatic acquisition поддерживаемым.

Overwolf также не одобряет background-only/private bridge и требует публичные UI-функции и monetization plan. Готовый bilingual draft заявки и список решений владельца находятся в [пакете заявки](OVERWOLF_PROPOSAL.md).

## Граница процессов

```text
Warframe
  -> Overwolf GEP: match_info.inventory
  -> отдельный одобренный companion
  -> локальный JSON v1
  -> bounded parser PlatScope
  -> resolver + atomic SQLite LKG
```

Companion не вызывает IPC PlatScope, не открывает его базу и не отправляет inventory на сервер. Для записи локального export используется документированный [`overwolf.io.writeFileContents`](https://dev.overwolf.com/ow-native/reference/io/ow-io/#writefilecontentsfilepath-content-encoding-triggeruacifrequired-callback) с manifest permission `FileSystem`. Путь выбирается явно; скрытое сканирование произвольных каталогов не требуется.

## Envelope v1

Машиночитаемая схема: [`schemas/overwolf-companion-inventory-v1.schema.json`](../schemas/overwolf-companion-inventory-v1.schema.json).

```json
{
  "schemaVersion": 1,
  "producer": "platscope-overwolf-companion",
  "observedAt": "2026-08-27T10:15:30Z",
  "gameId": 8954,
  "feature": "match_info",
  "key": "inventory",
  "complete": true,
  "value": {
    "Inventory": {
      "MiscItems": [
        {
          "ItemType": "/Lotus/Types/Items/Example",
          "ItemCount": 2
        }
      ]
    }
  }
}
```

`value` допускает объект или JSON-строку, потому что GEP wrappers встречаются в обеих формах. Parser требует точные producer/game/feature/key, известную schema version, валидный UTC timestamp и `complete: true`. Unknown fields в envelope запрещены. Общие границы остаются прежними: 8 МиБ, 100 000 сгруппированных строк, 64 уровня вложенности, 250 000 JSON nodes и checked quantities.

Производитель должен записывать только coherent snapshot. Включённый пользователем desktop poller проверяет файл раз в три секунды и требует одинаковые path/size/mtime в двух последовательных проходах. Неизменившийся невалидный sample проверяется только один раз; следующая попытка выполняется после изменения файла. Невалидная или частично записанная версия никогда не заменяет предыдущий LKG.

## Privacy и sellability

Envelope содержит только timestamp, технические идентификаторы события и inventory value. Запрещены username, chat, `accountId`, nonce, WFM token, memory fragments и advertising identifiers.

GEP inventory подтверждает владение и количество, но не доказывает tradeability каждого экземпляра. Поэтому все строки companion получают `Tradeability::Unknown`, имеют `sellable = 0` и не попадают в Sell Now автоматически. Изменить эту границу можно только при появлении отдельного надёжного сигнала tradeability и тестов на ложноположительные рекомендации.

## Что уже реализовано

- отдельный собираемый Native WebApp package `apps/overwolf-companion` с независимой версией `0.1.0`;
- manifest v1 с dedicated game targeting `8954`, видимым desktop window и только `GameInfo`/`FileSystem`;
- регистрация только `match_info` с тремя bounded attempts и проверкой `supportedFeatures`;
- fail-closed extraction documented event/getInfo wrappers, coherent validation и сохранение предыдущего snapshot при ошибке;
- bilingual RU/EN health-dashboard: Overwolf, Warframe, GEP, timestamp, строки, уникальные items и category totals;
- явный opt-in, проверка абсолютного `.json`-пути, ручная повторная синхронизация и UTF-8 export без UAC escalation;
- CSP `connect-src 'none'` и build-validator, отклоняющий сетевые примитивы в production bundle;
- официальный Overwolf manifest schema validation и PNG dock icon 256×256 менее 30 КБ;
- отдельный read-only workflow artifact `PlatScope-overwolf-companion-unpacked`, который не выдаётся за OPK;
- общий synthetic fixture `fixtures/inventory/overwolf_companion_v1.json`, который принимает Rust desktop parser;
- строгий auto-detection envelope v1 в `platscope-inventory`;
- отдельный источник `overwolf_companion` в domain, SQLite и frontend;
- сохранение `observedAt` вместо времени ручного импорта;
- fail-closed проверки неполного snapshot и неверного game/feature/key;
- opt-in настройки абсолютного пути и фоновый poller со stability check;
- ручная команда «Проверить файл» и событие обновления открытого Inventory UI;
- automatic boundary принимает только companion envelope; helper JSON остаётся ручным импортом;
- UI-предупреждение о неизвестной tradeability;
- Rust и frontend tests.

## Внешний gate до automatic acquisition

1. Подать и получить approval app idea у Overwolf.
2. Загрузить собранный `apps/overwolf-companion/dist` в whitelisted Overwolf account как unpacked extension.
3. Проверить реальный GEP payload на текущем Warframe build и зафиксировать fixture без персональных данных.
4. Провести отдельные privacy, EULA, packaging и distribution reviews.
5. Только после этих проверок создать и распространять reviewed OPK.

Принимающая desktop-часть и source package производителя готовы, но до выполнения этих пунктов нет одобренного real-GEP доказательства и end-to-end automatic acquisition не считается поддерживаемым.
