#![forbid(unsafe_code)]

use chrono::{Duration, NaiveDate};
use platscope_domain::MarketHistoryPoint;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MIN_DAILY_CLOSED_VOLUME: f64 = 3.0;
pub const MIN_POINTS_7D: usize = 3;
pub const MIN_POINTS_30D: usize = 7;
pub const MIN_POINTS_90D: usize = 14;
/// Для решения SELL/HOLD нужна как минимум треть календарных дней 90-дневного окна.
pub const MIN_TIMING_POINTS_90D: usize = 30;
/// Данные должны покрывать начало и конец окна, а не лежать только в его середине.
pub const MAX_TIMING_EDGE_GAP_DAYS: i64 = 14;
const MEANINGFUL_90D_CHANGE_PERCENT: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingSignal {
    Hold,
    Neutral,
    Sell,
    Peak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendSummary {
    pub median_7d: Option<f64>,
    pub median_30d: Option<f64>,
    pub median_90d: Option<f64>,
    pub change_7d: Option<f64>,
    pub change_30d: Option<f64>,
    pub change_90d: Option<f64>,
    pub volume_avg_7d: Option<f64>,
    pub volume_avg_30d: Option<f64>,
    pub volume_avg_90d: Option<f64>,
    pub historical_low: Option<f64>,
    pub historical_high: Option<f64>,
    pub timing: Option<TimingSignal>,
    pub trusted_days: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct TrendContext {
    pub as_of: NaiveDate,
    pub current_price: Option<f64>,
    pub live_lowest_ask: Option<f64>,
}

/// Рассчитывает volume-aware 7/30/90 day trend, не усиливая дни с микроскопическим объёмом.
pub fn calculate(points: &[MarketHistoryPoint], context: TrendContext) -> TrendSummary {
    let seven = summarize_window(points, context.as_of, 7, MIN_POINTS_7D);
    let thirty = summarize_window(points, context.as_of, 30, MIN_POINTS_30D);
    let ninety = summarize_window(points, context.as_of, 90, MIN_POINTS_90D);
    let price_change_90d = summarize_price_change_90d(points, context.as_of);
    let range = ninety.as_ref().or(thirty.as_ref()).or(seven.as_ref());
    let (historical_low, historical_high) =
        range.map_or((None, None), |window| (Some(window.low), Some(window.high)));
    let timing = ninety.as_ref().and_then(|window| {
        price_change_90d.and_then(|change| {
            timing_signal(
                context.current_price,
                context.live_lowest_ask,
                Some(window.low),
                Some(window.high),
                Some(change),
            )
        })
    });

    TrendSummary {
        median_7d: seven.as_ref().map(|window| window.median),
        median_30d: thirty.as_ref().map(|window| window.median),
        median_90d: ninety.as_ref().map(|window| window.median),
        change_7d: seven.as_ref().and_then(|window| window.change),
        change_30d: thirty.as_ref().and_then(|window| window.change),
        change_90d: price_change_90d,
        volume_avg_7d: calendar_average_volume(points, context.as_of, 7),
        volume_avg_30d: calendar_average_volume(points, context.as_of, 30),
        volume_avg_90d: calendar_average_volume(points, context.as_of, 90),
        historical_low,
        historical_high,
        timing,
        trusted_days: trusted_points_in_window(points, context.as_of, 90).len(),
    }
}

fn summarize_price_change_90d(points: &[MarketHistoryPoint], as_of: NaiveDate) -> Option<f64> {
    let first_date = as_of - Duration::days(89);
    let trusted = trusted_points_in_window(points, as_of, 90);
    if trusted.len() < MIN_TIMING_POINTS_90D {
        return None;
    }
    let first_observation = trusted.first()?.0;
    let last_observation = trusted.last()?.0;
    if first_observation
        .signed_duration_since(first_date)
        .num_days()
        > MAX_TIMING_EDGE_GAP_DAYS
        || as_of.signed_duration_since(last_observation).num_days() > MAX_TIMING_EDGE_GAP_DAYS
    {
        return None;
    }

    // Обычная линейная регрессия использует каждый надёжный календарный день окна
    // с одинаковым весом. Так середина периода влияет на направление тренда, а
    // единичный день с огромным объёмом не подменяет собой остальные дни.
    let count = u32::try_from(trusted.len()).ok()?;
    let n = f64::from(count);
    let (day_total, price_total, squared_day_total, cross_product_total) = trusted.iter().fold(
        (0.0, 0.0, 0.0, 0.0),
        |(day_total, price_total, squared_day_total, cross_product_total), (date, price, _)| {
            let x = i32::try_from(date.signed_duration_since(first_date).num_days())
                .map_or(0.0, f64::from);
            (
                day_total + x,
                price_total + price,
                squared_day_total + x * x,
                cross_product_total + x * price,
            )
        },
    );
    let denominator = n * squared_day_total - day_total * day_total;
    if denominator.abs() <= f64::EPSILON {
        return None;
    }
    let slope = (n * cross_product_total - day_total * price_total) / denominator;
    let intercept = (price_total - slope * day_total) / n;
    let fitted_start = intercept;
    let fitted_end = intercept + slope * 89.0;
    (fitted_start.is_finite() && fitted_end.is_finite() && fitted_start > 0.0 && fitted_end > 0.0)
        .then_some(((fitted_end - fitted_start) / fitted_start) * 100.0)
}

struct WindowSummary {
    median: f64,
    change: Option<f64>,
    low: f64,
    high: f64,
}

fn summarize_window(
    points: &[MarketHistoryPoint],
    as_of: NaiveDate,
    days: i64,
    minimum_points: usize,
) -> Option<WindowSummary> {
    let trusted = trusted_points_in_window(points, as_of, days);
    if trusted.len() < minimum_points {
        return None;
    }
    let median = weighted_median(&trusted)?;
    let (low, high) = robust_price_range(&trusted)?;
    let first = trusted.first()?.1;
    let last = trusted.last()?.1;
    let change = (first > 0.0).then_some(((last - first) / first) * 100.0);
    Some(WindowSummary {
        median,
        change,
        low,
        high,
    })
}

fn robust_price_range(points: &[(NaiveDate, f64, f64)]) -> Option<(f64, f64)> {
    let mut prices = points.iter().map(|point| point.1).collect::<Vec<_>>();
    if prices.is_empty() {
        return None;
    }
    prices.sort_by(f64::total_cmp);
    let last = prices.len() - 1;
    // Диапазон 10–90-го процентилей не даёт одному ошибочному дню превратить
    // нормальную текущую цену в ложный сигнал «ждать» или «пик».
    let low_index = last / 10;
    let high_index = last.saturating_sub(low_index);
    Some((prices[low_index], prices[high_index]))
}

fn trusted_points_in_window(
    points: &[MarketHistoryPoint],
    as_of: NaiveDate,
    days: i64,
) -> Vec<(NaiveDate, f64, f64)> {
    let first_date = as_of - Duration::days(days - 1);
    let mut daily = BTreeMap::<NaiveDate, (f64, f64)>::new();
    for point in points
        .iter()
        .filter(|point| point.source_date >= first_date && point.source_date <= as_of)
    {
        let Some(price) = trusted_price(point) else {
            continue;
        };
        let candidate = (price, point.closed_volume);
        daily
            .entry(point.source_date)
            .and_modify(|current| {
                if candidate.1 > current.1 {
                    *current = candidate;
                }
            })
            .or_insert(candidate);
    }
    daily
        .into_iter()
        .map(|(date, (price, volume))| (date, price, volume))
        .collect()
}

fn calendar_average_volume(
    points: &[MarketHistoryPoint],
    as_of: NaiveDate,
    days: i64,
) -> Option<f64> {
    let first_date = as_of - Duration::days(days - 1);
    let mut daily = BTreeMap::<NaiveDate, f64>::new();
    for point in points
        .iter()
        .filter(|point| point.source_date >= first_date && point.source_date <= as_of)
        .filter(|point| point.closed_volume.is_finite() && point.closed_volume >= 0.0)
    {
        daily
            .entry(point.source_date)
            .and_modify(|volume| *volume = volume.max(point.closed_volume))
            .or_insert(point.closed_volume);
    }
    let calendar_days = u32::try_from(days).ok()?;
    (!daily.is_empty()).then(|| daily.values().sum::<f64>() / f64::from(calendar_days))
}

fn trusted_price(point: &MarketHistoryPoint) -> Option<f64> {
    point.closed_median.filter(|price| {
        price.is_finite()
            && *price > 0.0
            && point.closed_volume.is_finite()
            && point.closed_volume >= MIN_DAILY_CLOSED_VOLUME
    })
}

fn weighted_median(points: &[(NaiveDate, f64, f64)]) -> Option<f64> {
    let mut weighted: Vec<(f64, f64)> = points
        .iter()
        .map(|(_, price, volume)| (*price, *volume))
        .collect();
    weighted.sort_by(|left, right| left.0.total_cmp(&right.0));
    let half = weighted.iter().map(|point| point.1).sum::<f64>() / 2.0;
    let mut cumulative = 0.0;
    for (price, volume) in weighted {
        cumulative += volume;
        if cumulative >= half {
            return Some(price);
        }
    }
    None
}

fn timing_signal(
    current_price: Option<f64>,
    live_lowest_ask: Option<f64>,
    low: Option<f64>,
    high: Option<f64>,
    change_90d: Option<f64>,
) -> Option<TimingSignal> {
    let (current, low, high) = (current_price?, low?, high?);
    if !current.is_finite() || current <= 0.0 || high < low {
        return None;
    }
    if (high - low).abs() <= f64::EPSILON {
        return Some(TimingSignal::Neutral);
    }
    let position = ((current - low) / (high - low)).clamp(0.0, 1.0);
    if position <= 0.2 {
        return Some(TimingSignal::Hold);
    }
    if position >= 0.8 {
        let live_confirms = live_lowest_ask
            .filter(|ask| ask.is_finite() && *ask > 0.0)
            .is_some_and(|ask| ((ask - current) / current).abs() <= 0.2);
        return Some(if live_confirms {
            TimingSignal::Peak
        } else {
            TimingSignal::Sell
        });
    }
    if change_90d.is_some_and(|change| change >= MEANINGFUL_90D_CHANGE_PERCENT) {
        return Some(TimingSignal::Hold);
    }
    if position >= 0.35 && change_90d.is_some_and(|change| change <= -MEANINGFUL_90D_CHANGE_PERCENT)
    {
        return Some(TimingSignal::Sell);
    }
    if position >= 0.65 {
        Some(TimingSignal::Sell)
    } else {
        Some(TimingSignal::Neutral)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(day: u32, price: f64, volume: f64) -> MarketHistoryPoint {
        dated_point(
            NaiveDate::from_ymd_opt(2026, 8, day).expect("valid fixture date"),
            price,
            volume,
        )
    }

    fn dated_point(source_date: NaiveDate, price: f64, volume: f64) -> MarketHistoryPoint {
        MarketHistoryPoint {
            source_date,
            closed_median: Some(price),
            closed_volume: volume,
            sell_median: None,
            buy_median: None,
        }
    }

    fn linear_history(as_of: NaiveDate, start: f64, end: f64) -> Vec<MarketHistoryPoint> {
        let first_date = as_of - Duration::days(89);
        (0_i32..90)
            .map(|offset| {
                let progress = f64::from(offset) / 89.0;
                dated_point(
                    first_date + Duration::days(i64::from(offset)),
                    start + (end - start) * progress,
                    5.0,
                )
            })
            .collect()
    }

    fn assert_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("expected value");
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    #[test]
    fn low_volume_spike_does_not_create_a_trend() {
        let points = vec![point(20, 10.0, 1.0), point(21, 40.0, 2.0)];
        let trend = calculate(
            &points,
            TrendContext {
                as_of: NaiveDate::from_ymd_opt(2026, 8, 21).expect("date"),
                current_price: Some(40.0),
                live_lowest_ask: Some(40.0),
            },
        );
        assert_eq!(trend.median_7d, None);
        assert_eq!(trend.timing, None);
        assert_close(trend.volume_avg_7d, 3.0 / 7.0);
    }

    #[test]
    fn weighted_median_and_change_use_trusted_days() {
        let points = vec![
            point(20, 10.0, 5.0),
            point(21, 11.0, 20.0),
            point(22, 14.0, 5.0),
        ];
        let trend = calculate(
            &points,
            TrendContext {
                as_of: NaiveDate::from_ymd_opt(2026, 8, 22).expect("date"),
                current_price: Some(14.0),
                live_lowest_ask: Some(14.0),
            },
        );
        assert_eq!(trend.median_7d, Some(11.0));
        assert_eq!(trend.change_7d, Some(40.0));
        assert_eq!(trend.timing, None);
    }

    #[test]
    fn three_recent_days_never_create_a_ninety_day_timing_signal() {
        let points = vec![
            point(20, 10.0, 5.0),
            point(21, 20.0, 5.0),
            point(22, 30.0, 5.0),
        ];
        let trend = calculate(
            &points,
            TrendContext {
                as_of: NaiveDate::from_ymd_opt(2026, 8, 22).expect("date"),
                current_price: Some(30.0),
                live_lowest_ask: Some(30.0),
            },
        );
        assert_eq!(trend.median_7d, Some(20.0));
        assert_eq!(trend.change_90d, None);
        assert_eq!(trend.timing, None);
    }

    #[test]
    fn ninety_day_price_trend_uses_the_whole_window() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let points = linear_history(as_of, 10.0, 20.0);
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(15.0),
                live_lowest_ask: None,
            },
        );

        assert_close(trend.change_90d, 100.0);
        assert_eq!(trend.volume_avg_90d, Some(5.0));
    }

    #[test]
    fn observations_in_the_middle_affect_the_ninety_day_trend() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let points = (0_i32..90)
            .map(|offset| {
                let price = if (30..45).contains(&offset) {
                    30.0
                } else {
                    10.0
                };
                dated_point(first_date + Duration::days(i64::from(offset)), price, 5.0)
            })
            .collect::<Vec<_>>();
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(10.0),
                live_lowest_ask: None,
            },
        );

        assert!(trend.change_90d.is_some_and(|change| change < -5.0));
    }

    #[test]
    fn one_extreme_day_does_not_distort_the_timing_range() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let mut points = linear_history(as_of, 10.0, 20.0);
        points[45].closed_median = Some(1_000.0);
        points[45].closed_volume = 1_000.0;
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(20.0),
                live_lowest_ask: Some(20.0),
            },
        );

        assert!(trend.historical_high.is_some_and(|price| price < 25.0));
        assert_eq!(trend.timing, Some(TimingSignal::Peak));
    }

    #[test]
    fn rising_price_below_peak_recommends_waiting() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let points = linear_history(as_of, 10.0, 20.0);

        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(15.0),
                live_lowest_ask: Some(15.0),
            },
        );

        assert_close(trend.change_90d, 100.0);
        assert_eq!(trend.timing, Some(TimingSignal::Hold));
    }

    #[test]
    fn falling_price_above_the_low_recommends_selling() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let points = linear_history(as_of, 20.0, 10.0);

        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(15.0),
                live_lowest_ask: Some(15.0),
            },
        );

        assert_close(trend.change_90d, -50.0);
        assert_eq!(trend.timing, Some(TimingSignal::Sell));
    }

    #[test]
    fn flat_ninety_day_price_is_neutral() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let points = linear_history(as_of, 10.0, 10.0);
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(10.0),
                live_lowest_ask: Some(10.0),
            },
        );

        assert_close(trend.change_90d, 0.0);
        assert_eq!(trend.timing, Some(TimingSignal::Neutral));
    }

    #[test]
    fn calendar_average_counts_missing_and_low_volume_days() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let mut points = (0_i32..14)
            .map(|index| dated_point(first_date + Duration::days(i64::from(index * 6)), 10.0, 3.0))
            .collect::<Vec<_>>();
        points.push(dated_point(as_of, 20.0, 1.0));
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(10.0),
                live_lowest_ask: None,
            },
        );

        assert_close(trend.volume_avg_90d, 43.0 / 90.0);
        assert_eq!(trend.trusted_days, 14);
        assert_eq!(trend.timing, None);
    }

    #[test]
    fn timing_requires_coverage_at_both_edges_of_the_window() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let points = (0_i32..30)
            .map(|offset| {
                dated_point(
                    first_date + Duration::days(i64::from(offset + 30)),
                    10.0 + f64::from(offset),
                    5.0,
                )
            })
            .collect::<Vec<_>>();
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(20.0),
                live_lowest_ask: None,
            },
        );

        assert_eq!(trend.trusted_days, 30);
        assert_eq!(trend.change_90d, None);
        assert_eq!(trend.timing, None);
    }

    #[test]
    fn thirty_well_spread_trusted_days_are_enough_for_timing() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let points = (0_i32..30)
            .map(|index| {
                let offset = index * 3;
                dated_point(
                    first_date + Duration::days(i64::from(offset)),
                    10.0 + f64::from(offset) / 10.0,
                    5.0,
                )
            })
            .collect::<Vec<_>>();
        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(14.0),
                live_lowest_ask: None,
            },
        );

        assert_eq!(trend.trusted_days, MIN_TIMING_POINTS_90D);
        assert!(trend.change_90d.is_some());
        assert_eq!(trend.timing, Some(TimingSignal::Hold));
    }
}
