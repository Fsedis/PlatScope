#![forbid(unsafe_code)]

use chrono::NaiveDate;
use platscope_domain::{
    LiveOrderBook, LiveOrderSide, MarketItemKind, MarketOrderType, MarketRecord, MarketVariantKey,
    PriceConfidence, PriceFreshness, ProviderId, UserStatus,
};
use serde::{Deserialize, Serialize};

pub const MIN_TRUSTED_CLOSED_VOLUME: f64 = 3.0;
pub const THIN_MARKET_VOLUME: f64 = 5.0;
pub const FRESH_DAYS: i64 = 2;
pub const AGING_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy)]
pub struct PricingContext<'a> {
    pub key: &'a MarketVariantKey,
    pub item_kind: MarketItemKind,
    pub source_date: NaiveDate,
    pub as_of: NaiveDate,
    pub provider: ProviderId,
    pub source_is_fallback: bool,
    pub bulk_records: &'a [MarketRecord],
    pub live_order_book: Option<&'a LiveOrderBook>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceReasonCode {
    TrustedClosedTrades,
    ClosedVolumeTooLow,
    ConservativeSellAdjustment,
    SellOnlyFallback,
    RelicSellIgnored,
    NoExactVariant,
    LiveBookVariantMismatch,
    IsolatedAskIgnored,
    LiveClusterShift,
    ThinMarketProtection,
    LiveMarketAgreement,
    LiveMarketDisagreement,
    LiveTopBuy,
    NoLiveTopBuy,
    SourceFresh,
    SourceAging,
    SourceStale,
    SourceDateInvalid,
    FallbackProvider,
    RivenPricingUnsupported,
    InsufficientSignal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceReason {
    pub code: PriceReasonCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRecommendation {
    pub key: MarketVariantKey,
    pub provider: ProviderId,
    pub source_date: NaiveDate,
    pub fair_price: Option<f64>,
    pub list_price: Option<f64>,
    pub quick_sell: Option<f64>,
    pub lowest_ask: Option<f64>,
    pub depth_three: Option<f64>,
    pub depth_price: Option<f64>,
    pub closed_volume: Option<f64>,
    pub live_sell_order_count: u32,
    pub live_buy_order_count: u32,
    pub confidence: PriceConfidence,
    pub freshness: PriceFreshness,
    pub reasons: Vec<PriceReason>,
}

/// Рассчитывает объяснимую рекомендацию только для точного market variant.
#[must_use]
pub fn recommend(context: PricingContext<'_>) -> PriceRecommendation {
    let mut reasons = Vec::new();
    let freshness = classify_freshness(context.source_date, context.as_of);
    explain_freshness(freshness, context.source_date, &mut reasons);
    if context.source_is_fallback {
        push_reason(
            &mut reasons,
            PriceReasonCode::FallbackProvider,
            "Использован резервный источник. Проверьте текущие ордера перед публикацией.",
        );
    }
    if context.item_kind == MarketItemKind::Riven {
        return unsupported_riven_recommendation(context, freshness, reasons);
    }

    let exact_records: Vec<&MarketRecord> = context
        .bulk_records
        .iter()
        .filter(|record| record.key == *context.key)
        .collect();
    let closed = signal(&exact_records, MarketOrderType::Closed);
    let sell = signal(&exact_records, MarketOrderType::Sell);
    let trusted_closed = closed.filter(|record| record.volume >= MIN_TRUSTED_CLOSED_VOLUME);
    if exact_records.is_empty() {
        push_reason(
            &mut reasons,
            PriceReasonCode::NoExactVariant,
            "Для точного rank/subtype/варианта нет bulk-записей.",
        );
    } else if let Some(record) = closed
        && record.volume < MIN_TRUSTED_CLOSED_VOLUME
    {
        push_reason(
            &mut reasons,
            PriceReasonCode::ClosedVolumeTooLow,
            format!(
                "Закрытых сделок недостаточно: {:.0}, требуется не менее {:.0}.",
                record.volume, MIN_TRUSTED_CLOSED_VOLUME
            ),
        );
    }

    let (fair_price, fair_basis) =
        fair_price(context.item_kind, trusted_closed, sell, &mut reasons);
    let live = live_stats(
        context.key,
        context.live_order_book,
        fair_price,
        &mut reasons,
    );
    let list_price = listing_price(fair_price, trusted_closed, &live, &mut reasons);
    let quick_sell = live.top_buy;
    if let Some(price) = quick_sell {
        push_reason(
            &mut reasons,
            PriceReasonCode::LiveTopBuy,
            format!("Quick Sell основан на лучшем активном buy order: {price:.2}p."),
        );
    } else {
        push_reason(
            &mut reasons,
            PriceReasonCode::NoLiveTopBuy,
            "Активного live buy order нет; исторический buy signal не выдан за Quick Sell.",
        );
    }

    let confidence = confidence(
        fair_price,
        fair_basis,
        trusted_closed,
        freshness,
        context.source_is_fallback,
        &live,
    );
    if fair_price.is_none() && list_price.is_none() && quick_sell.is_none() {
        push_reason(
            &mut reasons,
            PriceReasonCode::InsufficientSignal,
            "Надёжного ценового сигнала нет; PlatScope не подставляет 0p или 1p.",
        );
    }

    PriceRecommendation {
        key: context.key.clone(),
        provider: context.provider,
        source_date: context.source_date,
        fair_price,
        list_price,
        quick_sell,
        lowest_ask: live.lowest_ask,
        depth_three: live.depth_three,
        depth_price: live.depth_five,
        closed_volume: closed.map(|record| record.volume),
        live_sell_order_count: live.sell_count,
        live_buy_order_count: live.buy_count,
        confidence,
        freshness,
        reasons,
    }
}

fn unsupported_riven_recommendation(
    context: PricingContext<'_>,
    freshness: PriceFreshness,
    mut reasons: Vec<PriceReason>,
) -> PriceRecommendation {
    push_reason(
        &mut reasons,
        PriceReasonCode::RivenPricingUnsupported,
        "Уникальный Riven roll нельзя оценивать обычной item median; отдельная Riven-модель ещё не даёт надёжной цены.",
    );
    PriceRecommendation {
        key: context.key.clone(),
        provider: context.provider,
        source_date: context.source_date,
        fair_price: None,
        list_price: None,
        quick_sell: None,
        lowest_ask: None,
        depth_three: None,
        depth_price: None,
        closed_volume: None,
        live_sell_order_count: 0,
        live_buy_order_count: 0,
        confidence: PriceConfidence::Unknown,
        freshness,
        reasons,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FairBasis {
    TrustedClosed,
    SellFallback,
    None,
}

fn fair_price(
    item_kind: MarketItemKind,
    closed: Option<&MarketRecord>,
    sell: Option<&MarketRecord>,
    reasons: &mut Vec<PriceReason>,
) -> (Option<f64>, FairBasis) {
    let closed_median = closed.and_then(|record| record.median);
    let sell_median = sell.and_then(|record| record.median);
    if let Some(closed_median) = closed_median {
        push_reason(
            reasons,
            PriceReasonCode::TrustedClosedTrades,
            format!(
                "Fair baseline подтверждён {:.0} закрытыми сделками: {closed_median:.2}p.",
                closed.map_or(0.0, |record| record.volume)
            ),
        );
        if item_kind == MarketItemKind::Standard
            && let Some(sell_median) = sell_median
        {
            let fair = closed_median.min(sell_median);
            if fair < closed_median {
                push_reason(
                    reasons,
                    PriceReasonCode::ConservativeSellAdjustment,
                    format!("Sell median {sell_median:.2}p консервативно ограничил baseline."),
                );
            }
            return (Some(fair), FairBasis::TrustedClosed);
        }
        return (Some(closed_median), FairBasis::TrustedClosed);
    }

    if item_kind == MarketItemKind::Relic {
        if sell_median.is_some() {
            push_reason(
                reasons,
                PriceReasonCode::RelicSellIgnored,
                "Bulk sell median реликвии не используется без подтверждённых closed trades.",
            );
        }
        return (None, FairBasis::None);
    }
    if let Some(sell_median) = sell_median {
        push_reason(
            reasons,
            PriceReasonCode::SellOnlyFallback,
            format!(
                "Завершённых сделок недостаточно; ориентир {sell_median:.2}p рассчитан по ордерам продавцов."
            ),
        );
        return (Some(sell_median), FairBasis::SellFallback);
    }
    (None, FairBasis::None)
}

#[derive(Debug, Default)]
struct LiveStats {
    lowest_ask: Option<f64>,
    credible_ask: Option<f64>,
    depth_three: Option<f64>,
    depth_five: Option<f64>,
    top_buy: Option<f64>,
    sell_count: u32,
    buy_count: u32,
    cluster_shift: bool,
    agrees_with_fair: bool,
    disagrees_with_fair: bool,
}

fn live_stats(
    key: &MarketVariantKey,
    book: Option<&LiveOrderBook>,
    fair: Option<f64>,
    reasons: &mut Vec<PriceReason>,
) -> LiveStats {
    let Some(book) = book else {
        return LiveStats::default();
    };
    if book.key != *key {
        push_reason(
            reasons,
            PriceReasonCode::LiveBookVariantMismatch,
            "Live order book относится к другому варианту и полностью исключён.",
        );
        return LiveStats::default();
    }

    let mut asks: Vec<(f64, u32)> = Vec::new();
    let mut buys = Vec::new();
    for order in &book.orders {
        if order.user_status == UserStatus::Offline
            || order.platinum == 0
            || order.quantity == 0
            || order.per_trade == 0
        {
            continue;
        }
        match order.side {
            LiveOrderSide::Sell => asks.push((f64::from(order.platinum), order.quantity)),
            LiveOrderSide::Buy => buys.push(f64::from(order.platinum)),
        }
    }
    asks.sort_by(|left, right| left.0.total_cmp(&right.0));
    buys.sort_by(f64::total_cmp);

    let lowest_ask = asks.first().map(|order| order.0);
    let depth_three = depth_average(&asks, 3);
    let depth_five = depth_average(&asks, 5);
    let mut credible_ask = lowest_ask;
    let mut isolated_lowest = false;
    let mut cluster_shift = false;
    if let (Some(fair), Some(lowest)) = (fair, lowest_ask)
        && asks.len() >= 3
    {
        let neighbors = mean(asks.iter().skip(1).take(4).map(|order| order.0));
        isolated_lowest =
            lowest < fair / 3.0 && neighbors.is_some_and(|neighbor| neighbor >= fair * 0.60);
        if isolated_lowest {
            credible_ask = asks.get(1).map(|order| order.0);
            push_reason(
                reasons,
                PriceReasonCode::IsolatedAskIgnored,
                format!("Одиночный ask {lowest:.2}p исключён как изолированный undercut."),
            );
        }
        cluster_shift = depth_five.is_some_and(|depth| depth < fair * 0.50);
        if cluster_shift {
            push_reason(
                reasons,
                PriceReasonCode::LiveClusterShift,
                "Кластер live asks существенно ниже bulk baseline; это похоже на сдвиг рынка, а не одиночный выброс.",
            );
        }
    }

    let agreement_price = if isolated_lowest {
        mean(asks.iter().skip(1).take(3).map(|order| order.0))
    } else {
        depth_three.or(credible_ask)
    };
    let agrees_with_fair = fair
        .zip(agreement_price)
        .is_some_and(|(fair, live)| live >= fair * 0.75 && live <= fair * 1.25);
    let disagrees_with_fair = fair
        .zip(agreement_price)
        .is_some_and(|(fair, live)| live < fair * 0.50 || live > fair * 2.0);
    if agrees_with_fair {
        push_reason(
            reasons,
            PriceReasonCode::LiveMarketAgreement,
            "Live order cluster согласуется с bulk baseline.",
        );
    } else if disagrees_with_fair {
        push_reason(
            reasons,
            PriceReasonCode::LiveMarketDisagreement,
            "Live order cluster заметно расходится с bulk baseline.",
        );
    }

    LiveStats {
        lowest_ask,
        credible_ask,
        depth_three,
        depth_five,
        top_buy: buys.last().copied(),
        sell_count: u32::try_from(asks.len()).unwrap_or(u32::MAX),
        buy_count: u32::try_from(buys.len()).unwrap_or(u32::MAX),
        cluster_shift,
        agrees_with_fair,
        disagrees_with_fair,
    }
}

fn listing_price(
    fair: Option<f64>,
    closed: Option<&MarketRecord>,
    live: &LiveStats,
    reasons: &mut Vec<PriceReason>,
) -> Option<f64> {
    if live.sell_count < 3 {
        return fair;
    }
    let candidate = if live.cluster_shift {
        live.depth_three
    } else {
        live.credible_ask
    }?;
    if let Some(fair) = fair {
        let volume = closed.map_or(0.0, |record| record.volume);
        if volume < THIN_MARKET_VOLUME && candidate > fair * 3.0 {
            push_reason(
                reasons,
                PriceReasonCode::ThinMarketProtection,
                format!(
                    "Ask {candidate:.2}p не повышает рекомендацию на тонком рынке; сохранён baseline {fair:.2}p."
                ),
            );
            return Some(fair);
        }
    }
    Some(candidate)
}

fn confidence(
    fair: Option<f64>,
    fair_basis: FairBasis,
    closed: Option<&MarketRecord>,
    freshness: PriceFreshness,
    source_is_fallback: bool,
    live: &LiveStats,
) -> PriceConfidence {
    let mut confidence = match fair_basis {
        FairBasis::TrustedClosed => PriceConfidence::Medium,
        FairBasis::SellFallback => PriceConfidence::Low,
        FairBasis::None if live.top_buy.is_some() || live.sell_count >= 3 => PriceConfidence::Low,
        FairBasis::None => PriceConfidence::Unknown,
    };
    if fair.is_some()
        && closed.is_some_and(|record| record.volume >= THIN_MARKET_VOLUME)
        && live.sell_count >= 3
        && live.agrees_with_fair
        && freshness == PriceFreshness::Fresh
        && !source_is_fallback
    {
        confidence = PriceConfidence::High;
    }
    if live.cluster_shift || live.disagrees_with_fair {
        confidence = PriceConfidence::Low;
    }
    if freshness == PriceFreshness::Aging || source_is_fallback {
        confidence = downgrade(confidence);
    }
    if matches!(freshness, PriceFreshness::Stale | PriceFreshness::Unknown) {
        confidence = match confidence {
            PriceConfidence::Unknown => PriceConfidence::Unknown,
            _ => PriceConfidence::Low,
        };
    }
    confidence
}

const fn downgrade(confidence: PriceConfidence) -> PriceConfidence {
    match confidence {
        PriceConfidence::High => PriceConfidence::Medium,
        PriceConfidence::Medium | PriceConfidence::Low => PriceConfidence::Low,
        PriceConfidence::Unknown => PriceConfidence::Unknown,
    }
}

#[must_use]
pub fn classify_freshness(source_date: NaiveDate, as_of: NaiveDate) -> PriceFreshness {
    let age = as_of.signed_duration_since(source_date).num_days();
    match age {
        0..=FRESH_DAYS => PriceFreshness::Fresh,
        3..=AGING_DAYS => PriceFreshness::Aging,
        age if age > AGING_DAYS => PriceFreshness::Stale,
        _ => PriceFreshness::Unknown,
    }
}

fn signal<'a>(
    records: &[&'a MarketRecord],
    order_type: MarketOrderType,
) -> Option<&'a MarketRecord> {
    records
        .iter()
        .filter(|record| record.order_type == order_type)
        .max_by(|left, right| left.volume.total_cmp(&right.volume))
        .copied()
}

fn depth_average(orders: &[(f64, u32)], target_units: u32) -> Option<f64> {
    let mut remaining = target_units;
    let mut units = 0_u32;
    let mut total = 0.0;
    for (price, quantity) in orders {
        if remaining == 0 {
            break;
        }
        let taken = (*quantity).min(remaining);
        total += *price * f64::from(taken);
        units = units.saturating_add(taken);
        remaining -= taken;
    }
    (units > 0).then(|| total / f64::from(units))
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0_u32), |(sum, count), value| {
        (sum + value, count.saturating_add(1))
    });
    (count > 0).then(|| sum / f64::from(count))
}

fn explain_freshness(
    freshness: PriceFreshness,
    source_date: NaiveDate,
    reasons: &mut Vec<PriceReason>,
) {
    let (code, message) = match freshness {
        PriceFreshness::Fresh => (
            PriceReasonCode::SourceFresh,
            format!("Bulk snapshot свежий: {source_date}."),
        ),
        PriceFreshness::Aging => (
            PriceReasonCode::SourceAging,
            format!("Bulk snapshot начинает устаревать: {source_date}."),
        ),
        PriceFreshness::Stale => (
            PriceReasonCode::SourceStale,
            format!("Bulk snapshot устарел: {source_date}."),
        ),
        PriceFreshness::Unknown => (
            PriceReasonCode::SourceDateInvalid,
            format!("Дата snapshot находится в будущем: {source_date}."),
        ),
    };
    push_reason(reasons, code, message);
}

fn push_reason(reasons: &mut Vec<PriceReason>, code: PriceReasonCode, message: impl Into<String>) {
    reasons.push(PriceReason {
        code,
        message: message.into(),
    });
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use platscope_domain::{LiveOrder, Platform};

    use super::*;

    #[derive(Debug, Deserialize)]
    struct GoldenScenario {
        name: String,
        kind: MarketItemKind,
        #[serde(default)]
        subtype: Option<String>,
        #[serde(default)]
        closed_median: Option<f64>,
        #[serde(default)]
        closed_volume: Option<f64>,
        #[serde(default)]
        sell_median: Option<f64>,
        #[serde(default)]
        sell_volume: Option<f64>,
        asks: Vec<u32>,
        #[serde(default)]
        top_buy: Option<u32>,
        #[serde(default)]
        expected_fair: Option<f64>,
        #[serde(default)]
        expected_list: Option<f64>,
        #[serde(default)]
        expected_quick: Option<f64>,
        expected_confidence: PriceConfidence,
    }

    fn key(rank: Option<u16>, subtype: Option<&str>) -> MarketVariantKey {
        MarketVariantKey::new("test_item", Platform::Pc, rank, subtype.map(str::to_owned))
            .expect("valid key")
    }

    fn record(
        key: &MarketVariantKey,
        order_type: MarketOrderType,
        median: f64,
        volume: f64,
    ) -> MarketRecord {
        MarketRecord {
            key: key.clone(),
            external_item_id: "item-id".into(),
            display_name_en: "Test Item".into(),
            observed_at: Utc.with_ymd_and_hms(2026, 8, 26, 0, 0, 0).unwrap(),
            order_type,
            median: Some(median),
            average: None,
            min_price: None,
            max_price: None,
            volume,
            raw_json: "{}".into(),
        }
    }

    fn order(side: LiveOrderSide, price: u32, status: UserStatus) -> LiveOrder {
        LiveOrder {
            side,
            platinum: price,
            quantity: 1,
            per_trade: 1,
            user_status: status,
        }
    }

    fn context<'a>(
        key: &'a MarketVariantKey,
        records: &'a [MarketRecord],
        book: Option<&'a LiveOrderBook>,
        kind: MarketItemKind,
    ) -> PricingContext<'a> {
        PricingContext {
            key,
            item_kind: kind,
            source_date: NaiveDate::from_ymd_opt(2026, 8, 26).unwrap(),
            as_of: NaiveDate::from_ymd_opt(2026, 8, 27).unwrap(),
            provider: ProviderId::RelicsRun,
            source_is_fallback: false,
            bulk_records: records,
            live_order_book: book,
        }
    }

    fn assert_price(actual: Option<f64>, expected: f64) {
        assert!((actual.expect("price exists") - expected).abs() < 0.01);
    }

    fn assert_optional_price(actual: Option<f64>, expected: Option<f64>, scenario: &str) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => assert!(
                (actual - expected).abs() < 0.01,
                "unexpected price in {scenario}: {actual} != {expected}"
            ),
            (None, None) => {}
            _ => panic!("price presence differs in {scenario}"),
        }
    }

    #[test]
    fn golden_scenarios_match_explainable_output() {
        let scenarios: Vec<GoldenScenario> = serde_json::from_str(include_str!(
            "../../../fixtures/pricing/golden_scenarios.json"
        ))
        .expect("golden fixtures parse");

        for scenario in scenarios {
            let key = key(None, scenario.subtype.as_deref());
            let mut records = Vec::new();
            if let Some(median) = scenario.closed_median {
                records.push(record(
                    &key,
                    MarketOrderType::Closed,
                    median,
                    scenario.closed_volume.unwrap_or_default(),
                ));
            }
            if let Some(median) = scenario.sell_median {
                records.push(record(
                    &key,
                    MarketOrderType::Sell,
                    median,
                    scenario.sell_volume.unwrap_or_default(),
                ));
            }
            let mut orders: Vec<LiveOrder> = scenario
                .asks
                .iter()
                .map(|price| order(LiveOrderSide::Sell, *price, UserStatus::Online))
                .collect();
            if let Some(price) = scenario.top_buy {
                orders.push(order(LiveOrderSide::Buy, price, UserStatus::InGame));
            }
            let book = (!orders.is_empty()).then(|| LiveOrderBook {
                key: key.clone(),
                fetched_at: Utc::now(),
                orders,
            });

            let result = recommend(context(&key, &records, book.as_ref(), scenario.kind));

            assert_optional_price(result.fair_price, scenario.expected_fair, &scenario.name);
            assert_optional_price(result.list_price, scenario.expected_list, &scenario.name);
            assert_optional_price(result.quick_sell, scenario.expected_quick, &scenario.name);
            assert_eq!(
                result.confidence, scenario.expected_confidence,
                "confidence differs in {}",
                scenario.name
            );
            assert!(
                !result.reasons.is_empty(),
                "missing reasons in {}",
                scenario.name
            );
        }
    }

    #[test]
    fn liquid_market_uses_closed_and_live_cluster() {
        let key = key(None, None);
        let records = vec![
            record(&key, MarketOrderType::Closed, 40.0, 46.0),
            record(&key, MarketOrderType::Sell, 42.0, 100.0),
        ];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: vec![
                order(LiveOrderSide::Sell, 39, UserStatus::InGame),
                order(LiveOrderSide::Sell, 40, UserStatus::Online),
                order(LiveOrderSide::Sell, 41, UserStatus::Online),
                order(LiveOrderSide::Sell, 42, UserStatus::Online),
                order(LiveOrderSide::Buy, 35, UserStatus::InGame),
            ],
        };

        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_price(result.fair_price, 40.0);
        assert_price(result.list_price, 39.0);
        assert_price(result.quick_sell, 35.0);
        assert_price(result.depth_three, 40.0);
        assert_price(result.depth_price, 40.5);
        assert_eq!(result.confidence, PriceConfidence::High);
    }

    #[test]
    fn depth_signals_are_quantity_weighted_and_separate() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Closed, 15.0, 20.0)];
        let mut first = order(LiveOrderSide::Sell, 10, UserStatus::Online);
        first.quantity = 2;
        let mut second = order(LiveOrderSide::Sell, 20, UserStatus::Online);
        second.quantity = 3;
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: vec![first, second],
        };

        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_price(result.depth_three, 40.0 / 3.0);
        assert_price(result.depth_price, 16.0);
    }

    #[test]
    fn troll_ask_does_not_become_listing_price() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Closed, 40.0, 20.0)];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: [1, 39, 40, 41, 42]
                .map(|price| order(LiveOrderSide::Sell, price, UserStatus::Online))
                .to_vec(),
        };

        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_price(result.lowest_ask, 1.0);
        assert_price(result.list_price, 39.0);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == PriceReasonCode::IsolatedAskIgnored)
        );
    }

    #[test]
    fn low_cluster_is_treated_as_market_shift() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Closed, 40.0, 20.0)];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: [10, 11, 11, 12, 12]
                .map(|price| order(LiveOrderSide::Sell, price, UserStatus::Online))
                .to_vec(),
        };

        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_price(result.list_price, 10.67);
        assert_eq!(result.confidence, PriceConfidence::Low);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == PriceReasonCode::LiveClusterShift)
        );
    }

    #[test]
    fn single_fantasy_ask_does_not_create_a_price() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Closed, 10.0, 1.0)];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: vec![order(LiveOrderSide::Sell, 100, UserStatus::Online)],
        };

        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_eq!(result.fair_price, None);
        assert_eq!(result.list_price, None);
        assert_price(result.lowest_ask, 100.0);
        assert_eq!(result.confidence, PriceConfidence::Unknown);
    }

    #[test]
    fn standard_item_can_use_sell_only_as_low_confidence_fallback() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Sell, 25.0, 30.0)];
        let result = recommend(context(&key, &records, None, MarketItemKind::Standard));

        assert_price(result.fair_price, 25.0);
        assert_eq!(result.confidence, PriceConfidence::Low);
    }

    #[test]
    fn normal_item_uses_conservative_minimum_of_closed_and_sell() {
        let key = key(None, None);
        let records = vec![
            record(&key, MarketOrderType::Closed, 40.0, 20.0),
            record(&key, MarketOrderType::Sell, 36.0, 30.0),
        ];
        let result = recommend(context(&key, &records, None, MarketItemKind::Standard));

        assert_price(result.fair_price, 36.0);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| { reason.code == PriceReasonCode::ConservativeSellAdjustment })
        );
    }

    #[test]
    fn thin_market_cluster_cannot_raise_sell_recommendation_unreasonably() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Sell, 10.0, 2.0)];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: [100, 101, 102]
                .map(|price| order(LiveOrderSide::Sell, price, UserStatus::Online))
                .to_vec(),
        };
        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_price(result.list_price, 10.0);
        assert_eq!(result.confidence, PriceConfidence::Low);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| { reason.code == PriceReasonCode::ThinMarketProtection })
        );
    }

    #[test]
    fn coherent_higher_cluster_can_raise_listing_without_rewriting_fair_price() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Closed, 40.0, 20.0)];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc::now(),
            orders: [50, 51, 52, 53, 54]
                .map(|price| order(LiveOrderSide::Sell, price, UserStatus::Online))
                .to_vec(),
        };
        let result = recommend(context(
            &key,
            &records,
            Some(&book),
            MarketItemKind::Standard,
        ));

        assert_price(result.fair_price, 40.0);
        assert_price(result.list_price, 50.0);
    }

    #[test]
    fn relic_never_uses_sell_only_as_fair_price() {
        let key = key(None, Some("radiant"));
        let records = vec![record(&key, MarketOrderType::Sell, 25.0, 30.0)];
        let result = recommend(context(&key, &records, None, MarketItemKind::Relic));

        assert_eq!(result.fair_price, None);
        assert_eq!(result.list_price, None);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == PriceReasonCode::RelicSellIgnored)
        );
    }

    #[test]
    fn riven_never_uses_standard_bulk_or_live_pricing() {
        let key = MarketVariantKey::new(
            "soma_riven_mod",
            platscope_domain::Platform::Pc,
            None,
            None::<String>,
        )
        .expect("Riven key");
        let records = vec![record(&key, MarketOrderType::Closed, 500.0, 100.0)];
        let book = LiveOrderBook {
            key: key.clone(),
            fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
            orders: vec![order(LiveOrderSide::Buy, 450, UserStatus::Online)],
        };
        let result = recommend(context(&key, &records, Some(&book), MarketItemKind::Riven));
        assert_eq!(result.fair_price, None);
        assert_eq!(result.list_price, None);
        assert_eq!(result.quick_sell, None);
        assert_eq!(result.lowest_ask, None);
        assert_eq!(result.confidence, PriceConfidence::Unknown);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == PriceReasonCode::RivenPricingUnsupported)
        );
        assert_eq!(
            serde_json::to_string(&PriceReasonCode::RivenPricingUnsupported)
                .expect("reason code serializes"),
            "\"riven_pricing_unsupported\""
        );
    }

    #[test]
    fn exact_rank_is_never_replaced_with_max_rank() {
        let owned_rank = key(Some(0), None);
        let max_rank = key(Some(10), None);
        let records = vec![record(&max_rank, MarketOrderType::Closed, 80.0, 20.0)];
        let result = recommend(context(
            &owned_rank,
            &records,
            None,
            MarketItemKind::Standard,
        ));

        assert_eq!(result.fair_price, None);
        assert!(
            result
                .reasons
                .iter()
                .any(|reason| reason.code == PriceReasonCode::NoExactVariant)
        );
    }

    #[test]
    fn exact_relic_refinement_is_never_replaced() {
        let intact = key(None, Some("intact"));
        let radiant = key(None, Some("radiant"));
        let records = vec![record(&radiant, MarketOrderType::Closed, 12.0, 20.0)];
        let result = recommend(context(&intact, &records, None, MarketItemKind::Relic));

        assert_eq!(result.fair_price, None);
    }

    #[test]
    fn offline_orders_and_mismatched_book_are_ignored() {
        let current_key = key(None, None);
        let other = key(Some(1), None);
        let records = vec![record(&current_key, MarketOrderType::Closed, 20.0, 10.0)];
        let offline_book = LiveOrderBook {
            key: current_key.clone(),
            fetched_at: Utc::now(),
            orders: vec![order(LiveOrderSide::Buy, 18, UserStatus::Offline)],
        };
        let offline = recommend(context(
            &current_key,
            &records,
            Some(&offline_book),
            MarketItemKind::Standard,
        ));
        assert_eq!(offline.quick_sell, None);

        let other_book = LiveOrderBook {
            key: other,
            fetched_at: Utc::now(),
            orders: vec![order(LiveOrderSide::Buy, 18, UserStatus::Online)],
        };
        let mismatched = recommend(context(
            &current_key,
            &records,
            Some(&other_book),
            MarketItemKind::Standard,
        ));
        assert_eq!(mismatched.quick_sell, None);
    }

    #[test]
    fn stale_and_fallback_sources_lower_confidence_independently() {
        let key = key(None, None);
        let records = vec![record(&key, MarketOrderType::Closed, 20.0, 10.0)];
        let mut stale_context = context(&key, &records, None, MarketItemKind::Standard);
        stale_context.as_of = NaiveDate::from_ymd_opt(2026, 9, 10).unwrap();
        let stale = recommend(stale_context);
        assert_eq!(stale.freshness, PriceFreshness::Stale);
        assert_eq!(stale.confidence, PriceConfidence::Low);

        let mut fallback_context = context(&key, &records, None, MarketItemKind::Standard);
        fallback_context.source_is_fallback = true;
        let fallback = recommend(fallback_context);
        assert_eq!(fallback.freshness, PriceFreshness::Fresh);
        assert_eq!(fallback.confidence, PriceConfidence::Low);
    }
}
