# Архитектура PlatScope

## Границы приложения

PlatScope состоит из Tauri desktop shell, Svelte-интерфейса, Rust workspace и отдельного Windows OCR-процесса. WebView не получает сетевые токены, реквизиты Warframe или прямой доступ к SQLite: все операции проходят через типизированные Tauri-команды.

## Структура workspace

| Каталог | Ответственность |
| --- | --- |
| `apps/desktop` | Svelte UI, dev mock и Tauri shell |
| `apps/reward-ocr` | захват окна Warframe и русское OCR наград |
| `platscope-domain` | общие типы и идентичность рыночного варианта |
| `platscope-providers` | HTTP-клиенты, transport DTO и нормализация внешних данных |
| `platscope-pricing` | чистый расчёт рекомендуемой цены |
| `platscope-trends` | тренд цены за 7/30/90 дней и момент продажи |
| `platscope-inventory` | bounded-нормализация ответа DE и exact resolver |
| `platscope-readonly-scan` | read-only поиск сессии Warframe и получение инвентаря |
| `platscope-selling` | очерёдность продажи и номинальная стоимость |
| `platscope-insights` | комплекты, реликвии и дукаты |
| `platscope-account` | чтение и подтверждаемые изменения аккаунта WFM |
| `platscope-storage` | SQLite, миграции и атомарные LKG-снимки |
| `platscope-core` | прикладные сервисы и объединение подсистем |

Зависимости направлены внутрь: transport-форматы не попадают в domain, UI не повторяет формулы pricing, а providers не пишут в SQLite самостоятельно.

## Основные потоки

### Рыночные данные

```text
HTTP provider -> bounded body -> transport parser -> validation
-> normalized catalog/snapshot -> SQLite transaction -> LKG
-> pricing/trends -> Tauri DTO -> UI
```

Текущий и исторический снимки используют один exact `MarketVariantKey`: `slug + platform + rank + subtype + stars`. Данные другого варианта не подставляются как fallback.

### Инвентарь

```text
явная команда UI -> read-only scan процесса -> временная session info
-> inventory.php + точный VendorInfo -> bounded parser
-> resolver по текущему каталогу -> SQLite LKG -> UI
```

Raw memory, raw JSON, `accountId` и `nonce` не сохраняются. Информация о надетых модах хранит только SHA-256-ключ физической копии, тип снаряжения и номер конфигурации.

### Награды реликвий

DBWIN и `EE.log` используются как сигналы появления экрана. Отдельный OCR-процесс захватывает только область окна Warframe, сопоставляет русский текст с локальным каталогом и возвращает JSON через `stdout`. Tauri строит карточки и управляет прозрачным click-through оверлеем.

### Торговая смена

Один tailer читает новые строки `EE.log`. State machine принимает событие только после success-маркера и сохраняет его локально. Изменение WFM-ордера выполняется отдельной командой после подтверждения пользователя.

## Хранение

SQLite создаётся в каталоге данных приложения. Миграции `migrations/0001`–`0012` применяются последовательно и идемпотентно. В базе находятся:

- нормализованный каталог и рыночные снимки;
- компактная история точных вариантов;
- последний resolved-инвентарь и размещения модов;
- настройки, health источников и журнал подтверждённых сделок;
- публичные метаданные игры и краткоживущий точный снимок Норы.

Новый внешний снимок становится текущим только внутри успешной транзакции. Ошибка загрузки или schema drift не уничтожает предыдущий LKG.

## Конкурентность

Сетевые обновления и тяжёлые SQLite-операции выполняются вне потока WebView. Одинаковые refresh-команды сериализуются собственными async-lock, но навигация и ввод остаются доступными. OCR использует отдельное WAL-чтение, чтобы обновление рынка не задерживало показ наград.

## Генерируемые ресурсы

Сборка формирует ресурсы в следующем порядке:

1. Rust build script копирует корневой `THIRD_PARTY_NOTICES.md` в bundle resources;
2. frontend pre-build публикует self-contained OCR в игнорируемый `resources/reward-ocr`;
3. OCR builder запускает self-test русской модели;
4. Vite собирает Svelte frontend в игнорируемый `dist`.

В Git хранятся только исходники, русская модель OCR, минимальный набор Windows-иконок и placeholder каталога OCR.
