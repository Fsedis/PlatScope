# Метаданные и аналитика Sets / Relics / Ducats / Riven / Mastery

## Назначение

Этап 9 добавляет отдельный pipeline игровых метаданных и контекстные представления:

- сколько полных Prime sets можно собрать и выгоднее ли продавать комплект или детали;
- ожидаемую рыночную стоимость наград реликвии с явным покрытием цен;
- fair platinum, ducats и plat/ducat для распознанных Prime parts.
- weapon disposition и общий множитель Riven без оценки уникального roll.
- требование мастерства предмета в Market Item Detail без подмены отсутствующих данных нулём.

Метаданные не являются ценами и не участвуют в bulk price refresh. Ошибка их источника не ломает Pricing Engine, Market Browser или Sell Now.

## Источник и границы загрузки

Основной источник — MIT-репозиторий [WFCD/warframe-items](https://github.com/WFCD/warframe-items). Production provider читает `Relics.json` и девять категорий предметов из `data/json`.

Ограничения transport layer:

- не более 12 документов за refresh; production-набор содержит 10;
- не более 32 MiB на один HTTP response;
- не более 96 MiB суммарно;
- connect/request timeout, проверка content type и bounded body;
- validation перед записью: непустые normalized rows, конечные вероятности, сумма reward chances в пределах 99–101% и mastery requirement в диапазоне 0–50.

Transport DTO остаётся внутри `platscope-providers`. В домен попадает только `GameMetadataSnapshot`.

## Нормализованная модель

Snapshot содержит:

- `PrimeSetDefinition`: canonical set slug/game ref, display name, vault status и компоненты с recipe quantity;
- `RelicDefinition`: canonical relic slug/game ref, refinement, vault status и награды с реальным шансом;
- `PrimePartMetadata`: canonical slug/game ref, ducats и vault status;
- `RivenDispositionDefinition`: имя и game ref оружия, категория, ordinal disposition 1–5 и точный `omegaAttenuation` multiplier;
- `GameItemDefinition`: canonical slug/game ref и целое `mastery_requirement` 0–50;
- source, `fetched_at`, schema version, counts и SHA-256.

Join выполняется по exact catalog identity. Display name не используется как ключ. Для исторического расхождения WFCD/WFM у blueprint Warframe-компонентов разрешён только явный alias `Component -> Blueprint`; произвольного fuzzy join нет.

Поддерживаемые refinement:

```text
Intact
Exceptional
Flawless
Radiant
```

Текущий vault status нормализуется как `available`, `vaulted` или `unknown`. Vaulting soon и Resurgence могут быть добавлены только после появления проверяемого источника и не выводятся из цены.

## Требование мастерства

WFCD-поле `masteryReq` нормализуется только для предмета, чей `uniqueName` точно сопоставлен текущему WFM-каталогу. Metadata schema v3 хранит определения отдельно от цен и не пытается выводить MR из имени или категории. `MR 0` является настоящим значением; если определения нет, Item Detail явно показывает «Нет данных» / `No data`.

Для быстрого Market Browser lookup миграция 9 публикует компактную таблицу `game_item_definitions`, привязанную к конкретному metadata snapshot. Поиск загружает только индекс `slug -> mastery_requirement`, а не разбирает полный JSON на каждую строку.

## Riven disposition

WFCD публикует `disposition` и `omegaAttenuation` в документах `Primary`, `Secondary`, `Melee`, `SentinelWeapons`, `Arch-Gun` и `Arch-Melee`. Provider принимает только ordinal 1–5 и конечный multiplier в защищённом диапазоне 0.1–2.0; incoherent значение отклоняет весь кандидат до LKG promotion. Metadata schema v2 добавила эти определения, schema v3 — определения предметов с MR; старые snapshots читаются с безопасными пустыми массивами новых полей.

Вкладка Riven показывает количество оружия, средний/минимальный/максимальный multiplier и локальный поиск. Список ограничен первыми 200 совпадениями до уточнения запроса. Это общая статистика оружия, а не оценка мода: roll stats, rank, polarity и положительные/отрицательные свойства не смешиваются с ordinary item median.

## LKG и миграции 7–9

Таблица `game_metadata_snapshots` хранит целый normalized snapshot и отдельные counts/checksum. Миграция 8 добавляет `riven_disposition_count`, а миграция 9 — `item_definition_count` и компактный индекс `game_item_definitions`; оба count-поля имеют безопасный default 0 для существующих баз. `promote_game_metadata` сначала проверяет соответствие всех counts фактическим массивам, затем одной транзакцией снимает current pointer, сохраняет проекцию MR и публикует новый snapshot.

При network/schema/validation failure:

1. текущий metadata LKG остаётся неизменным;
2. `GameMetadataService` возвращает LKG с `stale=true` и предупреждением;
3. если LKG ещё нет, UI показывает безопасное пустое состояние;
4. price snapshot и остальные разделы продолжают работать.

После offline-first startup общий background scheduler проверяет metadata LKG независимо от price freshness. Если snapshot отсутствует или `fetched_at` старше 24 часов, WFCD pipeline запускается после доступности локального каталога. Ручной и фоновый refresh объединены одним async lock. После успешной atomic promotion событие `game-metadata-updated` обновляет открытый экран аналитики; failure не очищает текущий view.

## Prime sets

Количество комплектов определяется точными recipe quantities:

```text
complete_sets = min(floor(sellable_part_quantity / required_quantity))
```

Сравниваются:

```text
set fair value
sum(part fair × required quantity)
set premium
confidence/liquidity-adjusted value обоих вариантов
```

Liquidity factor насыщается как `volume / (volume + 10)`, confidence multiplier равен `1.0 / 0.75 / 0.4 / 0.0` для High / Medium / Low / Unknown. Рекомендация `set` или `parts` появляется только при преимуществе более 5%; иначе варианты считаются сопоставимыми. Если нет полного комплекта или всех credible prices, это отдельные состояния, а отсутствующие цены не заменяются нулём.

## Relic EV

Расчёт выполняется по фактическим chance percent выбранного refinement:

```text
priced EV = Σ(chance / 100 × credible fair reward price)
```

Reward без market slug, fair price или confidence не получает фиктивные `1p`. Его шанс остаётся в total coverage, но не входит в priced EV.

Состояния покрытия:

- `complete`: ценами покрыто не менее 99% вероятности;
- `partial`: покрыто не менее 50%, UI показывает только подтверждённую часть EV;
- `insufficient`: покрыто менее 50%, числовой EV скрыт.

Partial EV не нормализуется до 100% и сопровождается количеством неоценённых наград.

## Ducats

Для принадлежащих пользователю Prime parts рассчитывается:

```text
plat_per_ducat = fair_price / ducats
```

Показатель считается credible только при положительном fair price, положительном количестве ducats и confidence High/Medium. Low ask сам по себе не создаёт рекомендацию. UI прямо сообщает, что plat/ducat — сравнительная метрика, а решение о продаже игрокам или обмене у Baro остаётся за пользователем.

## UI и explainability

Раздел «Аналитика» содержит вкладки «Комплекты», «Реликвии», «Дукаты» и «Riven», summary counts источника, дату snapshot и явные empty/error/LKG states.

- Sets показывают recipe quantities, sellable quantities, fair set, сумму деталей, premium и причины решения.
- Relics показывают refinement, fair реликвии, priced EV, процент покрытия и цену каждой reward.
- Ducats показывают sellable quantity, fair, ducats, plat/ducat и credibility.
- Riven показывает searchable weapon disposition/multiplier и предупреждение, что это не цена roll.
- Vault status всегда сопровождается предупреждением: `vaulted` не означает обязательный рост цены.
- Market Item Detail показывает MR для точного canonical slug; отсутствие определения обозначается текстом, а не `MR 0`.

## Проверки

- pure edge-case tests для recipe quantities, illiquid set premium, partial/insufficient EV и low-confidence ducats;
- offline provider fixtures для set/relic normalization и explicit blueprint alias;
- provider fixture для точного Riven disposition/multiplier и count invariant;
- provider fixture и boundary test для MR, включая допустимый ноль и отклонение значения выше 50;
- storage test: невалидный snapshot сохраняет предыдущий LKG;
- production smoke: реальный catalog + bulk price + WFCD metadata + Market MR projection + inventory + InsightsView;
- frontend tests, Svelte diagnostics, semantic browser QA и проверка консоли.
