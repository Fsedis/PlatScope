#![forbid(unsafe_code)]

use platscope_domain::PriceConfidence;
use platscope_trends::TimingSignal;
use serde::{Deserialize, Serialize};

const QUANTITY_SATURATION: f64 = 5.0;
const PRICE_HALF_SATURATION: f64 = 50.0;
const VOLUME_HALF_SATURATION: f64 = 10.0;

#[derive(Debug, Clone, Copy)]
pub struct SellPriorityInput {
    pub sellable_quantity: u32,
    pub fair_price: Option<f64>,
    pub closed_volume: Option<f64>,
    pub confidence: PriceConfidence,
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
    pub confidence_multiplier: f64,
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
    let closed_volume = trusted_non_negative(input.closed_volume).unwrap_or(0.0);
    let quantity = (f64::from(input.sellable_quantity) / QUANTITY_SATURATION).min(1.0);
    let price = fair_price.map_or(0.0, |value| value / (value + PRICE_HALF_SATURATION));
    let liquidity = closed_volume / (closed_volume + VOLUME_HALF_SATURATION);
    let confidence_multiplier = confidence_multiplier(input.confidence);
    let timing_multiplier = timing_multiplier(input.timing);
    let factors = SellPriorityFactors {
        quantity,
        price,
        liquidity,
        confidence_multiplier,
        timing_multiplier,
    };

    let raw_score = if input.sellable_quantity == 0 || fair_price.is_none() {
        0.0
    } else {
        (0.25 * quantity + 0.35 * price + 0.40 * liquidity)
            * confidence_multiplier
            * timing_multiplier
            * 100.0
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
        return vec!["Нет подтверждённого количества для продажи; priority равен 0.".into()];
    }
    if trusted_positive(input.fair_price).is_none() {
        return vec!["Цена не рассчитана; предмет не поднимается в очереди продажи.".into()];
    }
    vec![
        format!(
            "Количество для продажи: {}; quantity-фактор {:.0}% с насыщением после 5 копий.",
            input.sellable_quantity,
            factors.quantity * 100.0
        ),
        format!(
            "Fair price и закрытые сделки дают price-фактор {:.0}% и liquidity-фактор {:.0}%.",
            factors.price * 100.0,
            factors.liquidity * 100.0
        ),
        format!(
            "Полнота рыночных данных и момент продажи учтены с весом {:.0}% и {:.0}%; итоговая очерёдность {score}/100.",
            factors.confidence_multiplier * 100.0,
            factors.timing_multiplier * 100.0
        ),
        "Priority — относительный порядок проверки, а не прогноз платины в день.".into(),
    ]
}

fn trusted_positive(value: Option<f64>) -> Option<f64> {
    value.filter(|number| number.is_finite() && *number > 0.0)
}

fn trusted_non_negative(value: Option<f64>) -> Option<f64> {
    value.filter(|number| number.is_finite() && *number >= 0.0)
}

const fn confidence_multiplier(confidence: PriceConfidence) -> f64 {
    match confidence {
        PriceConfidence::High => 1.0,
        PriceConfidence::Medium => 0.75,
        PriceConfidence::Low => 0.4,
        PriceConfidence::Unknown => 0.0,
    }
}

const fn timing_multiplier(timing: Option<TimingSignal>) -> f64 {
    match timing {
        Some(TimingSignal::Hold) => 0.45,
        Some(TimingSignal::Neutral) => 0.75,
        Some(TimingSignal::Sell) => 1.0,
        Some(TimingSignal::Peak) => 1.05,
        None => 0.65,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        quantity: u32,
        fair: Option<f64>,
        volume: f64,
        confidence: PriceConfidence,
        timing: TimingSignal,
    ) -> SellPriorityInput {
        SellPriorityInput {
            sellable_quantity: quantity,
            fair_price: fair,
            closed_volume: Some(volume),
            confidence,
            timing: Some(timing),
        }
    }

    #[test]
    fn missing_price_never_creates_priority_or_nominal_value() {
        let result = calculate_priority(input(
            100,
            None,
            500.0,
            PriceConfidence::High,
            TimingSignal::Peak,
        ));
        assert_eq!(result.score, 0);
        assert_eq!(result.band, SellPriorityBand::None);
        assert_eq!(nominal_value(100, None), None);
    }

    #[test]
    fn large_illiquid_stack_stays_below_liquid_prime_item() {
        let cheap_stack = calculate_priority(input(
            200,
            Some(5.0),
            0.2,
            PriceConfidence::Medium,
            TimingSignal::Neutral,
        ));
        let liquid_prime = calculate_priority(input(
            1,
            Some(40.0),
            20.0,
            PriceConfidence::High,
            TimingSignal::Sell,
        ));
        assert!(liquid_prime.score > cheap_stack.score);
    }

    #[test]
    fn hold_reduces_otherwise_equal_priority() {
        let hold = calculate_priority(input(
            2,
            Some(80.0),
            30.0,
            PriceConfidence::High,
            TimingSignal::Hold,
        ));
        let sell = calculate_priority(input(
            2,
            Some(80.0),
            30.0,
            PriceConfidence::High,
            TimingSignal::Sell,
        ));
        assert!(sell.score > hold.score);
    }

    #[test]
    fn unknown_confidence_prevents_false_priority() {
        let result = calculate_priority(input(
            5,
            Some(100.0),
            100.0,
            PriceConfidence::Unknown,
            TimingSignal::Peak,
        ));
        assert_eq!(result.score, 0);
    }

    #[test]
    fn nominal_value_is_quantity_times_fair_price_only() {
        assert_eq!(nominal_value(3, Some(42.5)), Some(127.5));
    }
}
