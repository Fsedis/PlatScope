#![forbid(unsafe_code)]

use chrono::{Duration, NaiveDate};
use platscope_domain::MarketHistoryPoint;
use serde::{Deserialize, Serialize};

pub const MIN_DAILY_CLOSED_VOLUME: f64 = 3.0;
pub const MIN_POINTS_7D: usize = 3;
pub const MIN_POINTS_30D: usize = 7;
pub const MIN_POINTS_90D: usize = 14;
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
    let timing = timing_signal(
        context.current_price,
        context.live_lowest_ask,
        historical_low,
        historical_high,
        price_change_90d,
    );

    TrendSummary {
        median_7d: seven.as_ref().map(|window| window.median),
        median_30d: thirty.as_ref().map(|window| window.median),
        median_90d: ninety.as_ref().map(|window| window.median),
        change_7d: seven.as_ref().and_then(|window| window.change),
        change_30d: thirty.as_ref().and_then(|window| window.change),
        change_90d: price_change_90d,
        volume_avg_7d: seven.as_ref().map(|window| window.average_volume),
        volume_avg_30d: thirty.as_ref().map(|window| window.average_volume),
        volume_avg_90d: ninety.as_ref().map(|window| window.average_volume),
        historical_low,
        historical_high,
        timing,
        trusted_days: points
            .iter()
            .filter(|point| trusted_price(point).is_some())
            .count(),
    }
}

fn summarize_price_change_90d(points: &[MarketHistoryPoint], as_of: NaiveDate) -> Option<f64> {
    let first_date = as_of - Duration::days(89);
    let early_end = first_date + Duration::days(29);
    let recent_start = as_of - Duration::days(29);

    let representative_price = |from: NaiveDate, through: NaiveDate| {
        let mut trusted = points
            .iter()
            .filter(|point| point.source_date >= from && point.source_date <= through)
            .filter_map(|point| {
                trusted_price(point).map(|price| (point.source_date, price, point.closed_volume))
            })
            .collect::<Vec<_>>();
        if trusted.len() < MIN_POINTS_30D {
            return None;
        }
        trusted.sort_by_key(|point| point.0);
        weighted_median(&trusted)
    };

    let early = representative_price(first_date, early_end)?;
    let recent = representative_price(recent_start, as_of)?;
    (early > 0.0).then_some(((recent - early) / early) * 100.0)
}

struct WindowSummary {
    median: f64,
    change: Option<f64>,
    average_volume: f64,
    low: f64,
    high: f64,
}

fn summarize_window(
    points: &[MarketHistoryPoint],
    as_of: NaiveDate,
    days: i64,
    minimum_points: usize,
) -> Option<WindowSummary> {
    let first_date = as_of - Duration::days(days - 1);
    let mut trusted: Vec<(NaiveDate, f64, f64)> = points
        .iter()
        .filter(|point| point.source_date >= first_date && point.source_date <= as_of)
        .filter_map(|point| {
            trusted_price(point).map(|price| (point.source_date, price, point.closed_volume))
        })
        .collect();
    if trusted.len() < minimum_points {
        return None;
    }
    trusted.sort_by_key(|point| point.0);
    let median = weighted_median(&trusted)?;
    let point_count = u32::try_from(trusted.len()).ok()?;
    let average_volume = trusted.iter().map(|point| point.2).sum::<f64>() / f64::from(point_count);
    let low = trusted
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let high = trusted
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let first = trusted.first()?.1;
    let last = trusted.last()?.1;
    let change = (first > 0.0).then_some(((last - first) / first) * 100.0);
    Some(WindowSummary {
        median,
        change,
        average_volume,
        low,
        high,
    })
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
    if !current.is_finite() || current <= 0.0 || high <= low {
        return None;
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
        MarketHistoryPoint {
            source_date: NaiveDate::from_ymd_opt(2026, 8, day).expect("valid fixture date"),
            closed_median: Some(price),
            closed_volume: volume,
            sell_median: None,
            buy_median: None,
        }
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
        assert_eq!(trend.timing, Some(TimingSignal::Peak));
    }

    #[test]
    fn peak_requires_live_confirmation() {
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
                live_lowest_ask: Some(10.0),
            },
        );
        assert_eq!(trend.timing, Some(TimingSignal::Sell));
    }

    #[test]
    fn ninety_day_price_trend_compares_early_and_recent_months() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let points = (0..90)
            .map(|offset| MarketHistoryPoint {
                source_date: first_date + Duration::days(offset),
                closed_median: Some(if offset < 30 {
                    10.0
                } else if offset >= 60 {
                    15.0
                } else {
                    12.0
                }),
                closed_volume: 5.0,
                sell_median: None,
                buy_median: None,
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

        assert_eq!(trend.change_90d, Some(50.0));
        assert_eq!(trend.volume_avg_90d, Some(5.0));
    }

    #[test]
    fn rising_price_below_peak_recommends_waiting() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let points = (0..90)
            .map(|offset| MarketHistoryPoint {
                source_date: first_date + Duration::days(offset),
                closed_median: Some(if offset < 30 {
                    10.0
                } else if offset < 60 {
                    20.0
                } else {
                    15.0
                }),
                closed_volume: 5.0,
                sell_median: None,
                buy_median: None,
            })
            .collect::<Vec<_>>();

        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(15.0),
                live_lowest_ask: Some(15.0),
            },
        );

        assert_eq!(trend.change_90d, Some(50.0));
        assert_eq!(trend.timing, Some(TimingSignal::Hold));
    }

    #[test]
    fn falling_price_above_the_low_recommends_selling() {
        let as_of = NaiveDate::from_ymd_opt(2026, 8, 29).expect("date");
        let first_date = as_of - Duration::days(89);
        let points = (0..90)
            .map(|offset| MarketHistoryPoint {
                source_date: first_date + Duration::days(offset),
                closed_median: Some(if offset < 30 {
                    20.0
                } else if offset < 60 {
                    10.0
                } else {
                    15.0
                }),
                closed_volume: 5.0,
                sell_median: None,
                buy_median: None,
            })
            .collect::<Vec<_>>();

        let trend = calculate(
            &points,
            TrendContext {
                as_of,
                current_price: Some(15.0),
                live_lowest_ask: Some(15.0),
            },
        );

        assert_eq!(trend.change_90d, Some(-25.0));
        assert_eq!(trend.timing, Some(TimingSignal::Sell));
    }
}
