#![forbid(unsafe_code)]

use platscope_domain::PriceConfidence;
use serde::{Deserialize, Serialize};

const LIQUIDITY_HALF_SATURATION: f64 = 10.0;
const DECISION_MARGIN: f64 = 0.05;
const COMPLETE_COVERAGE_PERCENT: f64 = 99.0;
const PARTIAL_COVERAGE_PERCENT: f64 = 50.0;
const MAX_COMPLETE_REWARD_TABLE_PERCENT: f64 = 101.0;
const MAX_UNPRICED_COMPLETE_PERCENT: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct SetPartInput<'a> {
    pub slug: &'a str,
    pub required_quantity: u32,
    pub sellable_quantity: u32,
    pub fair_price: Option<f64>,
    pub closed_volume: Option<f64>,
    pub confidence: PriceConfidence,
}

#[derive(Debug, Clone, Copy)]
pub struct SetComparisonInput<'a> {
    pub set_slug: &'a str,
    pub set_fair_price: Option<f64>,
    pub set_closed_volume: Option<f64>,
    pub set_confidence: PriceConfidence,
    pub parts: &'a [SetPartInput<'a>],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetSaleMode {
    Set,
    Parts,
    Equivalent,
    InsufficientInventory,
    InsufficientPricing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetComparison {
    pub set_slug: String,
    pub complete_sets: u32,
    pub set_fair_value: Option<f64>,
    pub parts_fair_value: Option<f64>,
    pub set_liquidity_adjusted_value: Option<f64>,
    pub parts_liquidity_adjusted_value: Option<f64>,
    pub set_premium_percent: Option<f64>,
    pub recommended_mode: SetSaleMode,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RelicRewardInput<'a> {
    pub reward_slug: Option<&'a str>,
    pub chance_percent: f64,
    pub fair_price: Option<f64>,
    pub confidence: PriceConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicPricingCoverage {
    Complete,
    Partial,
    Insufficient,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicExpectedValue {
    pub priced_expected_value: Option<f64>,
    pub priced_chance_percent: f64,
    pub total_chance_percent: f64,
    pub missing_reward_count: usize,
    pub coverage: RelicPricingCoverage,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DucatEfficiency {
    pub fair_price: Option<f64>,
    pub ducats: u32,
    pub platinum_per_ducat: Option<f64>,
    pub credible: bool,
    pub reasons: Vec<String>,
}

#[must_use]
pub fn compare_set(input: SetComparisonInput<'_>) -> SetComparison {
    let complete_sets = input
        .parts
        .iter()
        .filter(|part| part.required_quantity > 0)
        .map(|part| part.sellable_quantity / part.required_quantity)
        .min()
        .unwrap_or(0);
    let set_fair_value = credible_price(input.set_fair_price, input.set_confidence);
    let parts_fair_value = input.parts.iter().try_fold(0.0, |total, part| {
        credible_price(part.fair_price, part.confidence)
            .map(|price| total + price * f64::from(part.required_quantity))
    });
    let set_liquidity_adjusted_value = liquidity_adjusted_price(
        set_fair_value,
        input.set_confidence,
        input.set_closed_volume,
    );
    let parts_liquidity_adjusted_value = input.parts.iter().try_fold(0.0, |total, part| {
        liquidity_adjusted_price(part.fair_price, part.confidence, part.closed_volume)
            .map(|price| total + price * f64::from(part.required_quantity))
    });
    let set_premium_percent = set_fair_value
        .zip(parts_fair_value)
        .and_then(|(set, parts)| (parts > 0.0).then_some((set - parts) / parts * 100.0));

    let recommended_mode = if complete_sets == 0 {
        SetSaleMode::InsufficientInventory
    } else {
        match (set_liquidity_adjusted_value, parts_liquidity_adjusted_value) {
            (Some(set), Some(parts)) if set > parts * (1.0 + DECISION_MARGIN) => SetSaleMode::Set,
            (Some(set), Some(parts)) if parts > set * (1.0 + DECISION_MARGIN) => SetSaleMode::Parts,
            (Some(_), Some(_)) => SetSaleMode::Equivalent,
            _ => SetSaleMode::InsufficientPricing,
        }
    };
    let reasons = explain_set(
        complete_sets,
        set_fair_value,
        parts_fair_value,
        set_liquidity_adjusted_value,
        parts_liquidity_adjusted_value,
        recommended_mode,
    );
    SetComparison {
        set_slug: input.set_slug.to_owned(),
        complete_sets,
        set_fair_value,
        parts_fair_value,
        set_liquidity_adjusted_value,
        parts_liquidity_adjusted_value,
        set_premium_percent,
        recommended_mode,
        reasons,
    }
}

#[must_use]
pub fn calculate_relic_ev(rewards: &[RelicRewardInput<'_>]) -> RelicExpectedValue {
    let mut priced_expected_value = 0.0;
    let mut priced_chance_percent = 0.0;
    let mut total_chance_percent = 0.0;
    let mut missing_reward_count = 0;
    for reward in rewards {
        let Some(chance) = valid_chance(reward.chance_percent) else {
            missing_reward_count += 1;
            continue;
        };
        total_chance_percent += chance;
        if let Some(price) = credible_price(reward.fair_price, reward.confidence) {
            priced_expected_value += chance / 100.0 * price;
            priced_chance_percent += chance;
        } else {
            missing_reward_count += 1;
        }
    }
    let reward_table_complete = (COMPLETE_COVERAGE_PERCENT..=MAX_COMPLETE_REWARD_TABLE_PERCENT)
        .contains(&total_chance_percent);
    let unpriced_chance_percent = (total_chance_percent - priced_chance_percent).max(0.0);
    let coverage = if reward_table_complete
        && priced_chance_percent >= COMPLETE_COVERAGE_PERCENT
        && unpriced_chance_percent <= MAX_UNPRICED_COMPLETE_PERCENT
    {
        RelicPricingCoverage::Complete
    } else if reward_table_complete && priced_chance_percent >= PARTIAL_COVERAGE_PERCENT {
        RelicPricingCoverage::Partial
    } else {
        RelicPricingCoverage::Insufficient
    };
    let value = (coverage != RelicPricingCoverage::Insufficient).then_some(priced_expected_value);
    let reasons = vec![
        format!(
            "Ценами покрыто {:.1}% вероятности из {:.1}% описанных наград.",
            priced_chance_percent, total_chance_percent
        ),
        format!("Неоценённых наград: {missing_reward_count}; они не заменены фиктивной ценой 1p."),
        "Partial EV показывает только подтверждённую часть и не нормализуется до 100%.".into(),
    ];
    RelicExpectedValue {
        priced_expected_value: value,
        priced_chance_percent,
        total_chance_percent,
        missing_reward_count,
        coverage,
        reasons,
    }
}

#[must_use]
pub fn calculate_ducat_efficiency(
    fair_price: Option<f64>,
    ducats: u32,
    confidence: PriceConfidence,
) -> DucatEfficiency {
    let fair_price = trusted_price(fair_price);
    let credible = ducats > 0
        && fair_price.is_some()
        && matches!(confidence, PriceConfidence::High | PriceConfidence::Medium);
    let platinum_per_ducat = credible.then(|| fair_price.unwrap_or_default() / f64::from(ducats));
    let reasons = if credible {
        vec![
            "Эффективность рассчитана по credible fair price, а не по единичному low ask.".into(),
            "Plat/ducat — сравнительный показатель; решение о Baro остаётся за пользователем."
                .into(),
        ]
    } else {
        vec![
            "Недостаточно credible pricing или ducat metadata; рекомендация не сформирована."
                .into(),
        ]
    };
    DucatEfficiency {
        fair_price,
        ducats,
        platinum_per_ducat,
        credible,
        reasons,
    }
}

fn explain_set(
    complete_sets: u32,
    set_fair: Option<f64>,
    parts_fair: Option<f64>,
    set_adjusted: Option<f64>,
    parts_adjusted: Option<f64>,
    mode: SetSaleMode,
) -> Vec<String> {
    let mut reasons = vec![format!(
        "Из текущих деталей можно собрать комплектов: {complete_sets}."
    )];
    if let (Some(set), Some(parts)) = (set_fair, parts_fair) {
        reasons.push(format!(
            "Fair set: {set:.1}p; сумма fair деталей: {parts:.1}p."
        ));
    } else {
        reasons.push("Не все fair prices доступны; отсутствующие цены не заменены нулём.".into());
    }
    if let (Some(set), Some(parts)) = (set_adjusted, parts_adjusted) {
        reasons.push(format!(
            "С учётом полноты цен и числа сделок: комплект {set:.1}p, детали {parts:.1}p."
        ));
    }
    reasons.push(match mode {
        SetSaleMode::Set => "С учётом ликвидности комплект выглядит сильнее.".into(),
        SetSaleMode::Parts => "С учётом ликвидности отдельные детали выглядят сильнее.".into(),
        SetSaleMode::Equivalent => "Разница не превышает 5%; оба варианта сопоставимы.".into(),
        SetSaleMode::InsufficientInventory => "Полный комплект пока нельзя собрать.".into(),
        SetSaleMode::InsufficientPricing => "Для сравнения не хватает credible prices.".into(),
    });
    reasons
}

fn trusted_price(value: Option<f64>) -> Option<f64> {
    value.filter(|price| price.is_finite() && *price > 0.0)
}

fn credible_price(value: Option<f64>, confidence: PriceConfidence) -> Option<f64> {
    matches!(confidence, PriceConfidence::High | PriceConfidence::Medium)
        .then(|| trusted_price(value))
        .flatten()
}

fn valid_chance(value: f64) -> Option<f64> {
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

fn liquidity_adjusted_price(
    price: Option<f64>,
    confidence: PriceConfidence,
    volume: Option<f64>,
) -> Option<f64> {
    let price = credible_price(price, confidence)?;
    let volume = volume.filter(|value| value.is_finite() && *value > 0.0)?;
    Some(
        price * confidence_multiplier(confidence) * (volume / (volume + LIQUIDITY_HALF_SATURATION)),
    )
}

const fn confidence_multiplier(confidence: PriceConfidence) -> f64 {
    match confidence {
        PriceConfidence::High => 1.0,
        PriceConfidence::Medium => 0.75,
        PriceConfidence::Low | PriceConfidence::Unknown => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_set_count_respects_recipe_quantities() {
        let parts = [
            SetPartInput {
                slug: "blade",
                required_quantity: 2,
                sellable_quantity: 5,
                fair_price: Some(10.0),
                closed_volume: Some(20.0),
                confidence: PriceConfidence::High,
            },
            SetPartInput {
                slug: "blueprint",
                required_quantity: 1,
                sellable_quantity: 3,
                fair_price: Some(20.0),
                closed_volume: Some(20.0),
                confidence: PriceConfidence::High,
            },
        ];
        let result = compare_set(SetComparisonInput {
            set_slug: "test_set",
            set_fair_price: Some(55.0),
            set_closed_volume: Some(20.0),
            set_confidence: PriceConfidence::High,
            parts: &parts,
        });
        assert_eq!(result.complete_sets, 2);
        assert_eq!(result.parts_fair_value, Some(40.0));
    }

    #[test]
    fn protected_copies_do_not_count_as_a_saleable_complete_set() {
        let parts = [SetPartInput {
            slug: "part",
            required_quantity: 1,
            sellable_quantity: 0,
            fair_price: Some(10.0),
            closed_volume: Some(20.0),
            confidence: PriceConfidence::High,
        }];
        let result = compare_set(SetComparisonInput {
            set_slug: "test_set",
            set_fair_price: Some(20.0),
            set_closed_volume: Some(20.0),
            set_confidence: PriceConfidence::High,
            parts: &parts,
        });

        assert_eq!(result.complete_sets, 0);
        assert_eq!(result.recommended_mode, SetSaleMode::InsufficientInventory);
    }

    #[test]
    fn illiquid_set_premium_does_not_automatically_win() {
        let parts = [SetPartInput {
            slug: "part",
            required_quantity: 1,
            sellable_quantity: 1,
            fair_price: Some(90.0),
            closed_volume: Some(100.0),
            confidence: PriceConfidence::High,
        }];
        let result = compare_set(SetComparisonInput {
            set_slug: "test_set",
            set_fair_price: Some(120.0),
            set_closed_volume: Some(0.2),
            set_confidence: PriceConfidence::Medium,
            parts: &parts,
        });
        assert_eq!(result.recommended_mode, SetSaleMode::Parts);
        assert!(
            result
                .set_premium_percent
                .is_some_and(|premium| premium > 0.0)
        );
    }

    #[test]
    fn unknown_set_confidence_is_insufficient_instead_of_zero_value() {
        let parts = [SetPartInput {
            slug: "part",
            required_quantity: 1,
            sellable_quantity: 1,
            fair_price: Some(10.0),
            closed_volume: Some(20.0),
            confidence: PriceConfidence::High,
        }];
        let result = compare_set(SetComparisonInput {
            set_slug: "test_set",
            set_fair_price: Some(50.0),
            set_closed_volume: Some(20.0),
            set_confidence: PriceConfidence::Unknown,
            parts: &parts,
        });

        assert_eq!(result.set_fair_value, None);
        assert_eq!(result.set_liquidity_adjusted_value, None);
        assert_eq!(result.recommended_mode, SetSaleMode::InsufficientPricing);
    }

    #[test]
    fn missing_or_zero_liquidity_is_insufficient_instead_of_zero_value() {
        for volume in [None, Some(0.0)] {
            let parts = [SetPartInput {
                slug: "part",
                required_quantity: 1,
                sellable_quantity: 1,
                fair_price: Some(10.0),
                closed_volume: Some(20.0),
                confidence: PriceConfidence::High,
            }];
            let result = compare_set(SetComparisonInput {
                set_slug: "test_set",
                set_fair_price: Some(50.0),
                set_closed_volume: volume,
                set_confidence: PriceConfidence::High,
                parts: &parts,
            });

            assert_eq!(result.set_liquidity_adjusted_value, None);
            assert_eq!(result.recommended_mode, SetSaleMode::InsufficientPricing);
        }
    }

    #[test]
    fn missing_reward_price_stays_partial_and_is_not_one_platinum() {
        let rewards = [
            RelicRewardInput {
                reward_slug: Some("priced"),
                chance_percent: 60.0,
                fair_price: Some(10.0),
                confidence: PriceConfidence::High,
            },
            RelicRewardInput {
                reward_slug: Some("missing"),
                chance_percent: 40.0,
                fair_price: None,
                confidence: PriceConfidence::Unknown,
            },
        ];
        let result = calculate_relic_ev(&rewards);
        assert_eq!(result.coverage, RelicPricingCoverage::Partial);
        assert_eq!(result.priced_expected_value, Some(6.0));
        assert_eq!(result.missing_reward_count, 1);
    }

    #[test]
    fn low_coverage_relic_does_not_expose_misleading_ev() {
        let rewards = [RelicRewardInput {
            reward_slug: Some("rare"),
            chance_percent: 10.0,
            fair_price: Some(100.0),
            confidence: PriceConfidence::High,
        }];
        let result = calculate_relic_ev(&rewards);
        assert_eq!(result.coverage, RelicPricingCoverage::Insufficient);
        assert_eq!(result.priced_expected_value, None);
    }

    #[test]
    fn malformed_reward_probability_table_never_exposes_ev() {
        let rewards = [RelicRewardInput {
            reward_slug: Some("duplicated"),
            chance_percent: 100.0,
            fair_price: Some(100.0),
            confidence: PriceConfidence::High,
        }; 2];
        let result = calculate_relic_ev(&rewards);

        assert!((result.total_chance_percent - 200.0).abs() < f64::EPSILON);
        assert_eq!(result.coverage, RelicPricingCoverage::Insufficient);
        assert_eq!(result.priced_expected_value, None);
    }

    #[test]
    fn low_confidence_reward_is_not_counted_as_priced_ev() {
        let rewards = [RelicRewardInput {
            reward_slug: Some("uncertain"),
            chance_percent: 100.0,
            fair_price: Some(100.0),
            confidence: PriceConfidence::Low,
        }];
        let result = calculate_relic_ev(&rewards);

        assert_eq!(result.priced_expected_value, None);
        assert!(result.priced_chance_percent.abs() < f64::EPSILON);
        assert_eq!(result.coverage, RelicPricingCoverage::Insufficient);
    }

    #[test]
    fn ducat_efficiency_requires_credible_fair_price() {
        let credible = calculate_ducat_efficiency(Some(20.0), 100, PriceConfidence::Medium);
        assert_eq!(credible.platinum_per_ducat, Some(0.2));
        let low = calculate_ducat_efficiency(Some(1.0), 100, PriceConfidence::Low);
        assert!(!low.credible);
        assert_eq!(low.platinum_per_ducat, None);
    }
}
