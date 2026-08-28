# PlatScope Companion

Отдельный Overwolf Native WebApp для поддерживаемого события Warframe GEP `match_info.inventory`. Companion показывает состояние игры и события, проверяет coherent snapshot и только после явного opt-in записывает локальный envelope v1 для PlatScope.

## Граница безопасности

- manifest запрашивает только `GameInfo` и `FileSystem`;
- game targeting ограничен Warframe ID `8954`;
- принимается только `match_info.inventory`;
- `username`, chat, account ID, nonce и WFM credentials не читаются;
- в bundle нет `fetch`, `XMLHttpRequest`, `WebSocket` или `sendBeacon`, а CSP задаёт `connect-src 'none'`;
- inventory не сохраняется в `localStorage`: там находятся только язык, opt-in и выбранный путь;
- новый payload проходит те же лимиты, что desktop parser: 8 МиБ, 100 000 строк, 64 уровня, 250 000 nodes, quantity до 1 000 000 и ItemType до 256 символов;
- невалидный payload не заменяет последний coherent snapshot;
- экспорт не доказывает tradeability, поэтому desktop импортирует строки как `Unknown` и не добавляет их в Sell Now автоматически.

## Локальные команды

```text
pnpm --dir apps/overwolf-companion check
pnpm --dir apps/overwolf-companion test
pnpm --dir apps/overwolf-companion build
```

`build` создаёт `apps/overwolf-companion/dist`, затем проверяет manifest, PNG icon, минимальные permissions, Warframe targeting, видимое desktop window, CSP и отсутствие сетевых примитивов в production bundle. После проверки создаётся `dist/SHA256SUMS.txt` для всех файлов unpacked package.

Для browser-QA собранного UI:

```text
pnpm --dir apps/overwolf-companion exec vite preview --host 127.0.0.1 --port 1421 --strictPort
```

Откройте `http://127.0.0.1:1421/?mock=1`. Mock повторяет только локальный Overwolf callback contract и не считается доказательством реального GEP.

## Загрузка в Overwolf

После approval/whitelist загрузите папку `dist` как unpacked extension. `manifest.json` находится в её корне. До approval package не распространяется как OPK и не называется поддерживаемым automatic acquisition.

Для реальной проверки нужны текущий Warframe build, whitelisted Overwolf account и очищенный от персональных данных fixture фактического `match_info.inventory`.
