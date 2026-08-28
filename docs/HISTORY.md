# История и Trend Engine

## Хранение

PlatScope не накапливает raw daily dumps. После bounded download, parsing и validation каждый день сворачивается до одной строки на exact variant:

```text
source_date
item_slug + platform + rank + subtype + amber/cyan stars
closed_median
closed_volume
sell_median
buy_median
```

Metadata дня хранит provider, checksum, fetched/imported timestamps и исходные item/record counts. Повторный импорт даты заменяет её в одной транзакции. Текущий bulk snapshot архивируется одновременно с promotion и поэтому доступен истории сразу.

## Background bootstrap

Окно и Market Browser запускаются до history network. После setup отдельная async task:

1. читает текущую дату snapshot и catalog из LKG;
2. проверяет даты назад до 90 дней;
3. пропускает уже импортированные дни;
4. загружает не более семи новых дней за запуск;
5. сохраняет только compact aggregates.

Ошибки background history пишутся в structured log и не делают локальный рынок недоступным. Пустая база также не является ошибкой: bootstrap начнётся после появления catalog и snapshot.

## Trend Engine

День считается надёжным при closed median выше нуля и closed volume не ниже `3`. Минимум точек:

```text
7d: 3
30d: 7
90d: 14
```

Медиана окна является volume-weighted median дневных closed medians. Change — изменение между первой и последней надёжной точкой окна; average volume считается только по надёжным дням. Если floor или minimum points не выполнены, соответствующий metric остаётся `null`.

Timing использует положение текущей fair price внутри самого длинного надёжного historical range:

- нижние 20% — `HOLD`;
- середина — `NEUTRAL`;
- верхняя часть — `SELL`;
- верхние 20% становятся `PEAK` только если live lowest ask находится в пределах 20% от fair price.

Это рекомендация времени проверки, а не финансовое предсказание.

## UI и доступность

Item detail запрашивает 7, 30 или 90 дней только для выбранного варианта. Линейный SVG показывает closed median, прямые подписи границ и дат. У SVG есть `<title>`/`<desc>`, а полный числовой ряд продублирован семантической таблицей для screen reader. Кнопки диапазона нативные, используют `aria-pressed`, сохраняют 40px target и помещаются без горизонтальной прокрутки на 320 px.

## Проверка

Production smoke 27 августа 2026 года импортировал семь новых дат за 17 секунд, получил покрытие восемь дней без ошибок и рассчитал для `secura_dual_cestra`: median 7d `30p`, change 7d `−1,67%`, timing `HOLD`.
