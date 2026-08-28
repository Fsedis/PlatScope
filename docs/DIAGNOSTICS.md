# Диагностика

Полноценный экран «Диагностика» читает только локальное состояние SQLite. Само открытие и кнопка «Обновить сведения» не выполняют сетевых запросов и не меняют last-known-good данные.

## Provider health

Для `relics.run`, `FrameForgePricing` и `Warframe.Market` показываются:

- последняя попытка;
- последний успешный ответ;
- latency последней попытки;
- число последовательных ошибок;
- ограниченный код и безопасное сообщение последней ошибки.

Статусы не выдумываются:

- `Работает` — последняя сохранённая попытка успешна;
- `Есть сохранённые данные` — последняя попытка завершилась ошибкой, но ранее был успех;
- `Недоступен` — ошибки есть, успешного ответа ещё не было;
- `Ещё не проверялся` — локальных фактов об источнике нет.

Bulk refresh обновляет health primary/mirror providers. Реальный live price request обновляет health WFM; попадание в TTL cache не маскируется под новый сетевой успех.

## Локальное покрытие

Экран показывает версию приложения и схемы, offline-ready состояние, путь локальной БД, metadata текущего рыночного snapshot, размер каталога, диапазон истории и число строк последнего корректного inventory snapshot.

## Безопасный экспорт

Кнопка «Экспортировать безопасный отчёт» — отдельное явное действие. Backend строит versioned allowlist DTO, записывает JSON через temporary file и atomic rename в локальную папку `PlatScope/diagnostics`, затем возвращает UI путь и размер.

В отчёт входят версия приложения/схемы, offline state, агрегаты market/catalog/history/inventory и provider health. Путь БД и содержимое локальных payload не сериализуются.

## Privacy boundary

Диагностический DTO и экспорт никогда не содержат WFM token, email/password, account ID, nonce, raw inventory, memory fragments, authorization headers и путь локальной БД. Сообщение provider-ошибки ограничивается до записи в SQLite.

## Проверки

- storage test проверяет успешное и неуспешное состояние provider;
- desktop test проверяет отсутствие database path и credential keys в serialized report;
- frontend tests проверяют `ok/degraded/error/unchecked` и обязательные три provider-карточки;
- browser QA проверяет семантику, отсутствие console errors и responsive layout;
- общий release gate остаётся formatter, clippy, Rust/frontend tests, Vite build и Tauri release smoke.
