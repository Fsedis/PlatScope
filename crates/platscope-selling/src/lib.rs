#![forbid(unsafe_code)]

use platscope_trends::TimingSignal;
use serde::{Deserialize, Serialize};

const QUANTITY_SATURATION: f64 = 5.0;
const PRICE_HALF_SATURATION: f64 = 50.0;
const VOLUME_HALF_SATURATION: f64 = 10.0;

#[derive(Debug, Clone, Copy)]
pub struct SellPriorityInput {
    pub sellable_quantity: u32,
    pub fair_price: Option<f64>,
    /// Среднее число закрытых сделок за календарный день устойчивого периода.
    /// Разовый объём последнего снимка сюда передавать нельзя.
    pub average_daily_volume: Option<f64>,
    pub timing: Option<TimingSignal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SellPriorityBand {
    None,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellPriorityFactors {
    pub quantity: f64,
    pub price: f64,
    pub liquidity: f64,
    pub timing_multiplier: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellPriorityScore {
    pub score: u8,
    pub band: SellPriorityBand,
    pub factors: SellPriorityFactors,
    pub reasons: Vec<String>,
}

/// Рассчитывает относительный ranking score 0..100. Результат не является прогнозом
/// платины в день и намеренно насыщает quantity, price и liquidity.
#[must_use]
pub fn calculate_priority(input: SellPriorityInput) -> SellPriorityScore {
    let fair_price = trusted_positive(input.fair_price);
    let average_daily_volume = trusted_non_negative(input.average_daily_volume).unwrap_or(0.0);
    let quantity = (f64::from(input.sellable_quantity) / QUANTITY_SATURATION).min(1.0);
    let price = fair_price.map_or(0.0, |value| value / (value + PRICE_HALF_SATURATION));
    let liquidity = average_daily_volume / (average_daily_volume + VOLUME_HALF_SATURATION);
    let timing_multiplier = timing_ceiling(input.timing) / 100.0;
    let factors = SellPriorityFactors {
        quantity,
        price,
        liquidity,
        timing_multiplier,
    };

    let market_score = 0.25 * quantity + 0.35 * price + 0.40 * liquidity;
    let raw_score = if input.sellable_quantity == 0 || fair_price.is_none() {
        0.0
    } else {
        score_in_timing_band(input.timing, market_score)
    };
    let score = bounded_score(raw_score);
    let band = match score {
        50..=u8::MAX => SellPriorityBand::High,
        25..=49 => SellPriorityBand::Medium,
        1..=24 => SellPriorityBand::Low,
        0 => SellPriorityBand::None,
    };
    let reasons = explain(input, &factors, score);
    SellPriorityScore {
        score,
        band,
        factors,
        reasons,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn bounded_score(raw_score: f64) -> u8 {
    // После round+clamp значение конечно и гарантированно находится в диапазоне u8.
    raw_score.round().clamp(0.0, 100.0) as u8
}

/// Номинальная стоимость отделена от priority и не учитывает скорость реализации объёма.
#[must_use]
pub fn nominal_value(sellable_quantity: u32, fair_price: Option<f64>) -> Option<f64> {
    trusted_positive(fair_price).map(|price| f64::from(sellable_quantity) * price)
}

fn explain(input: SellPriorityInput, factors: &SellPriorityFactors, score: u8) -> Vec<String> {
    if input.sellable_quantity == 0 {
        return vec!["Нет подтверждённого количества для продажи; приоритет равен 0.".into()];
    }
    if trusted_positive(input.fair_price).is_none() {
        return vec!["Цена не рассчитана; предмет не поднимается в очереди продажи.".into()];
    }
    vec![
        format!(
            "Для продажи доступно: {}; вклад количества {:.0}% с насыщением после 5 копий.",
            input.sellable_quantity,
            factors.quantity * 100.0
        ),
        format!(
            "Цена и средний дневной объём дают вклад цены {:.0}% и ликвидности {:.0}%.",
            factors.price * 100.0,
            factors.liquidity * 100.0
        ),
        format!(
            "Момент продажи задаёт диапазон приоритета до {:.0}, итог {score}/100.",
            factors.timing_multiplier * 100.0
        ),
        "Приоритет — относительный порядок проверки, а не прогноз платины в день.".into(),
    ]
}

fn trusted_positive(value: Option<f64>) -> Option<f64> {
    value.filter(|number| number.is_finite() && *number > 0.0)
}

fn trusted_non_negative(value: Option<f64>) -> Option<f64> {
    value.filter(|number| number.is_finite() && *number >= 0.0)
}

const fn timing_ceiling(timing: Option<TimingSignal>) -> f64 {
    match timing {
        Some(TimingSignal::Hold) => 19.0,
        None => 29.0,
        Some(TimingSignal::Neutral) => 49.0,
        Some(TimingSignal::Sell) => 89.0,
        Some(TimingSignal::Peak) => 100.0,
    }
}

fn score_in_timing_band(timing: Option<TimingSignal>, market_score: f64) -> f64 {
    let market_score = market_score.clamp(0.0, 1.0);
    let (floor, width) = match timing {
        Some(TimingSignal::Hold) => (1.0, 18.0),
        None => (10.0, 19.0),
        Some(TimingSignal::Neutral) => (30.0, 19.0),
        Some(TimingSignal::Sell) => (50.0, 39.0),
        Some(TimingSignal::Peak) => (60.0, 40.0),
    };
    floor + width * market_score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        quantity: u32,
        fair: Option<f64>,
        volume: f64,
        timing: TimingSignal,
    ) -> SellPriorityInput {
        SellPriorityInput {
            sellable_quantity: quantity,
            fair_price: fair,
            average_daily_volume: Some(volume),
            timing: Some(timing),
        }
    }

    #[test]
    fn missing_price_never_creates_priority_or_nominal_value() {
        let result = calculate_priority(input(100, None, 500.0, TimingSignal::Peak));
        assert_eq!(result.score, 0);
        assert_eq!(result.band, SellPriorityBand::None);
        assert_eq!(nominal_value(100, None), None);
    }

    #[test]
    fn large_illiquid_stack_stays_below_liquid_prime_item() {
        let cheap_stack = calculate_priority(input(200, Some(5.0), 0.2, TimingSignal::Neutral));
        let liquid_prime = calculate_priority(input(1, Some(40.0), 20.0, TimingSignal::Sell));
        assert!(liquid_prime.score > cheap_stack.score);
    }

    #[test]
    fn hold_reduces_otherwise_equal_priority() {
        let hold = calculate_priority(input(2, Some(80.0), 30.0, TimingSignal::Hold));
        let sell = calculate_priority(input(2, Some(80.0), 30.0, TimingSignal::Sell));
        assert!(sell.score > hold.score);
    }

    #[test]
    fn sell_always_ranks_above_hold_even_with_weaker_market_factors() {
        let strongest_hold =
            calculate_priority(input(100, Some(1_000.0), 1_000.0, TimingSignal::Hold));
        let weakest_sell = calculate_priority(input(1, Some(1.0), 0.0, TimingSignal::Sell));

        assert!(weakest_sell.score > strongest_hold.score);
        assert!(weakest_sell.score >= 50);
        assert!(strongest_hold.score <= 19);
    }

    #[test]
    fn liquidity_uses_period_average_daily_volume() {
        let sparse = calculate_priority(input(1, Some(50.0), 1.0, TimingSignal::Sell));
        let liquid = calculate_priority(input(1, Some(50.0), 20.0, TimingSignal::Sell));

        assert!(liquid.factors.liquidity > sparse.factors.liquidity);
        assert!(liquid.score > sparse.score);
    }

    #[test]
    fn nominal_value_is_quantity_times_fair_price_only() {
        assert_eq!(nominal_value(3, Some(42.5)), Some(127.5));
    }
}
