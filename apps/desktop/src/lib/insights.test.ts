import { describe, expect, it } from "vitest";

import {
  coverageLabel,
  filterAndSortOpportunitySets,
  filterAndSortSets,
  formatPercent,
  formatRatio,
  rankRelicsToOpen,
  reservePublishedSetListings,
  refinementLabel,
  setLiveMinimumPrice,
  setLiveSellOrders,
  setModeLabel,
  setOpportunity,
  setRelicSupport,
  type RelicInsightRow,
  type SetInsightRow,
  vaultLabel,
} from "./insights";
import type { LiveOrderView, PriceConfidence, PriceRecommendation } from "./market";

function recommendation(
  slug: string,
  fairPrice: number | null,
  confidence: PriceConfidence = "medium",
): PriceRecommendation {
  return {
    key: { slug, platform: "pc", rank: null, charges: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run",
    sourceDate: "2026-08-27",
    fairPrice,
    listPrice: fairPrice,
    quickSell: null,
    lowestAsk: fairPrice,
    depthThree: fairPrice,
    depthPrice: fairPrice,
    closedVolume: 20,
    liveSellOrderCount: 5,
    liveBuyOrderCount: 0,
    confidence,
    freshness: "fresh",
    reasons: [],
  };
}

function relicRow(rewardSlug: string, chancePercent = 10, ownedQuantity = 3): RelicInsightRow {
  return {
    displayName: "Реликвия Акси T1",
    definition: {
      relicSlug: "axi_t1_relic",
      relicGameRef: "/Lotus/Types/Game/Projections/AxiT1Bronze",
      displayNameEn: "Axi T1 Relic",
      refinement: "intact",
      vaultStatus: "available",
      rewards: [{
        rewardSlug,
        rewardGameRef: "/Lotus/Reward",
        displayNameEn: "Useful reward",
        chancePercent,
      }],
    },
    ownedQuantity,
    sellableQuantity: Math.max(0, ownedQuantity - 1),
    relicRecommendation: null,
    expectedValue: {
      pricedExpectedValue: null,
      pricedChancePercent: 0,
      totalChancePercent: 100,
      missingRewardCount: 1,
      coverage: "insufficient",
      reasons: [],
    },
    rewards: [{
      displayName: "Нужная деталь",
      definition: {
        rewardSlug,
        rewardGameRef: "/Lotus/Reward",
        displayNameEn: "Useful reward",
        chancePercent,
      },
      recommendation: null,
    }],
  };
}

function pricedRelicRow(
  relicSlug: string,
  refinement: RelicInsightRow["definition"]["refinement"],
  rareRewardSlug: string,
  rarePrice = 40,
): RelicInsightRow {
  const chances = refinement === "intact"
    ? [25.33, 25.33, 25.33, 11, 11, 2]
    : refinement === "exceptional"
      ? [23.33, 23.33, 23.33, 13, 13, 4]
      : refinement === "flawless"
        ? [20, 20, 20, 17, 17, 6]
        : [16.67, 16.67, 16.67, 20, 20, 10];
  const specs = [
    { slug: `${relicSlug}_common_a`, price: 1 },
    { slug: `${relicSlug}_common_b`, price: 1 },
    { slug: `${relicSlug}_common_c`, price: 1 },
    { slug: `${relicSlug}_uncommon_a`, price: 5 },
    { slug: `${relicSlug}_uncommon_b`, price: 5 },
    { slug: rareRewardSlug, price: rarePrice },
  ];
  const rewards = specs.map((spec, index) => ({
    definition: {
      rewardSlug: spec.slug,
      rewardGameRef: `/Lotus/${spec.slug}`,
      displayNameEn: spec.slug,
      chancePercent: chances[index],
    },
    displayName: spec.slug,
    recommendation: recommendation(spec.slug, spec.price),
  }));
  return {
    definition: {
      relicSlug,
      relicGameRef: `/Lotus/Types/Game/Projections/${relicSlug}`,
      displayNameEn: relicSlug,
      refinement,
      vaultStatus: "available",
      rewards: rewards.map((reward) => reward.definition),
    },
    displayName: relicSlug,
    ownedQuantity: 2,
    sellableQuantity: 1,
    relicRecommendation: recommendation(relicSlug, 2),
    expectedValue: {
      pricedExpectedValue: null,
      pricedChancePercent: 100,
      totalChancePercent: 100,
      missingRewardCount: 0,
      coverage: "complete",
      reasons: [],
    },
    rewards,
  };
}

function setRow(
  name: string,
  missing: number,
  completeSets = 0,
  setFair = 100,
  partsFair = 80,
): SetInsightRow {
  const slug = name.toLocaleLowerCase().replaceAll(" ", "_");
  return {
    itemId: `${slug}-id`,
    displayName: name,
    definition: {
      setSlug: slug,
      setGameRef: `/Lotus/${slug}`,
      displayNameEn: name,
      vaultStatus: "available",
      components: [
        { slug: `${slug}_a`, gameRef: "/Lotus/A", requiredQuantity: 1, ducats: 45 },
        { slug: `${slug}_b`, gameRef: "/Lotus/B", requiredQuantity: 1, ducats: 45 },
      ],
    },
    setRecommendation: recommendation(slug, setFair),
    comparison: {
      setSlug: slug,
      completeSets,
      setFairValue: setFair,
      partsFairValue: partsFair,
      setLiquidityAdjustedValue: setFair,
      partsLiquidityAdjustedValue: partsFair,
      setPremiumPercent: (setFair - partsFair) / partsFair * 100,
      recommendedMode: completeSets > 0 ? "set" : "insufficient_inventory",
      reasons: [],
    },
    components: [
      {
        itemId: `${slug}_a-id`,
        displayName: `${name} A`,
        definition: { slug: `${slug}_a`, gameRef: "/Lotus/A", requiredQuantity: 1, ducats: 45 },
        ownedQuantity: completeSets + (missing === 0 ? 1 : 0),
        sellableQuantity: completeSets + (missing === 0 ? 1 : 0),
        recommendation: recommendation(`${slug}_a`, 30),
      },
      {
        itemId: `${slug}_b-id`,
        displayName: `${name} B`,
        definition: { slug: `${slug}_b`, gameRef: "/Lotus/B", requiredQuantity: 1, ducats: 45 },
        ownedQuantity: completeSets + (missing < 2 ? 1 : 0),
        sellableQuantity: completeSets + (missing < 2 ? 1 : 0),
        recommendation: recommendation(`${slug}_b`, 50),
      },
    ],
  };
}

describe("insights presentation", () => {
  it("uses the five cheapest executable sell orders for a live set price", () => {
    const orders: LiveOrderView[] = [
      { side: "sell", platinum: 25, quantity: 1, perTrade: 1, userStatus: "online" },
      { side: "sell", platinum: 42, quantity: 4, perTrade: 2, userStatus: "in_game" },
      { side: "sell", platinum: 20, quantity: 1, perTrade: 1, userStatus: "in_game" },
      { side: "sell", platinum: 22, quantity: 2, perTrade: 1, userStatus: "online" },
      { side: "sell", platinum: 23, quantity: 1, perTrade: 1, userStatus: "online" },
      { side: "sell", platinum: 24, quantity: 1, perTrade: 1, userStatus: "online" },
      { side: "sell", platinum: 1, quantity: 1, perTrade: 1, userStatus: "offline" },
      { side: "buy", platinum: 30, quantity: 1, perTrade: 1, userStatus: "in_game" },
      { side: "sell", platinum: 10, quantity: 1, perTrade: 2, userStatus: "in_game" },
    ];

    const visible = setLiveSellOrders(orders);

    expect(visible.map((order) => order.pricePerSet)).toEqual([20, 21, 22, 23, 24]);
    expect(visible[1]).toMatchObject({ quantity: 4, perTrade: 2, userStatus: "in_game" });
    expect(setLiveMinimumPrice(orders)).toBe(20);
  });

  it("does not invent a live set price without an active sell order", () => {
    expect(setLiveMinimumPrice([
      { side: "buy", platinum: 30, quantity: 1, perTrade: 1, userStatus: "in_game" },
      { side: "sell", platinum: 20, quantity: 1, perTrade: 1, userStatus: "offline" },
    ])).toBeNull();
  });

  it("явно различает incomplete analytics states", () => {
    expect(setModeLabel("insufficient_pricing")).toBe("Не хватает цен");
    expect(coverageLabel("partial")).toBe("Цены есть для части наград");
    expect(formatRatio(null)).toBe("—");
  });

  it("не превращает vaulted в ценовой прогноз", () => {
    expect(vaultLabel("vaulted")).toBe("В хранилище");
    expect(vaultLabel("unknown")).toBe("Статус неизвестен");
  });

  it("форматирует refinement и premium без потери знака", () => {
    expect(refinementLabel("radiant")).toBe("Сияющая");
    expect(formatPercent(12.34)).toBe("+12,3%");
    expect(formatPercent(-5)).toBe("-5%");
  });

  it("localizes typed insight states without translating canonical values", () => {
    expect(setModeLabel("insufficient_pricing", "en")).toBe("Not enough prices");
    expect(coverageLabel("partial", "en")).toBe("Partial EV");
    expect(refinementLabel("radiant", "en")).toBe("Radiant");
  });

  it("calculates the next set without treating owned parts as free profit", () => {
    const opportunity = setOpportunity(setRow("Nyx Prime Set", 1));
    expect(opportunity.missingQuantity).toBe(1);
    expect(opportunity.completionCost).toBe(30);
    expect(opportunity.setPremiumValue).toBe(20);
    expect(opportunity.ownedPartsOpportunityValue).toBe(50);
    expect(opportunity.completionProfit).toBe(20);
    expect(opportunity.profitableToComplete).toBe(true);
  });

  it("uses the conservative three-unit depth when two identical parts are required", () => {
    const row = setRow("Dual Blade Set", 1);
    row.definition.components[0].requiredQuantity = 2;
    row.components[0].definition.requiredQuantity = 2;
    row.components[0].ownedQuantity = 0;

    const opportunity = setOpportunity(row);

    expect(opportunity.missingQuantity).toBe(2);
    expect(opportunity.missingParts[0].buyPrice).toBe(30);
    expect(opportunity.missingParts[0].costBasis).toBe("depth_3");
    expect(opportunity.completionCost).toBe(60);
    expect(opportunity.completionProfit).toBe(-10);
    expect(opportunity.profitableToComplete).toBe(false);
  });

  it("uses a fresh credible market estimate when live depth was not requested", () => {
    const row = setRow("Bulk Estimate Set", 1);
    const missing = row.components[0].recommendation!;
    missing.lowestAsk = null;
    missing.depthThree = null;
    missing.depthPrice = null;
    missing.liveSellOrderCount = 0;

    const opportunity = setOpportunity(row);

    expect(opportunity.missingParts[0].buyPrice).toBe(30);
    expect(opportunity.missingParts[0].costBasis).toBe("market_estimate");
    expect(opportunity.completionCost).toBe(30);
  });

  it("separates profitable completion candidates from ready sets", () => {
    const easy = setRow("Easy Set", 1);
    const harder = setRow("Hard Set", 2);
    const ready = setRow("Ready Set", 0, 1);
    expect(filterAndSortSets([harder, ready, easy], "finish").map((row) => row.displayName))
      .toEqual(["Easy Set", "Hard Set"]);
    expect(filterAndSortSets([easy, ready], "ready")).toEqual([ready]);
  });

  it("does not offer a set order when its parts are protected by the reserve", () => {
    const ready = setRow("Reserved Set", 0, 1);
    for (const component of ready.components) component.sellableQuantity = 0;

    const opportunity = setOpportunity(ready);
    expect(opportunity.completeSets).toBe(1);
    expect(opportunity.sellableCompleteSets).toBe(0);
    expect(opportunity.missingQuantity).toBe(2);
    expect(opportunity.ownedPartsOpportunityValue).toBe(0);
    expect(filterAndSortOpportunitySets([ready], [], "ready")).toEqual([]);
  });

  it("never presents protected or untradeable parts as free set-completion stock", () => {
    const row = setRow("Protected Profit Set", 1, 0, 100, 80);
    row.components[1].ownedQuantity = 1;
    row.components[1].sellableQuantity = 0;

    const opportunity = setOpportunity(row);

    expect(opportunity.missingParts.map((part) => part.slug)).toEqual([
      row.components[0].definition.slug,
      row.components[1].definition.slug,
    ]);
    expect(opportunity.completionCost).toBe(80);
    expect(opportunity.completionProfit).toBe(20);
  });

  it("does not reuse parts already reserved by published market orders", () => {
    const row = setRow("Reserved Set", 0);
    for (const component of row.components) component.sellableQuantity = 2;
    const reserved = reservePublishedSetListings(row, [{
      itemId: row.components[0].itemId ?? null,
      type: "sell",
      quantity: 2,
      visible: true,
      rank: null,
      charges: null,
      subtype: null,
      amberStars: null,
      cyanStars: null,
    }]);

    expect(reserved.components[0].sellableQuantity).toBe(0);
    expect(setOpportunity(reserved).sellableCompleteSets).toBe(0);
  });

  it("reserves a shared component for an order on another set", () => {
    const target = setRow("Target Set", 0);
    const other = setRow("Other Set", 0);
    for (const component of target.components) component.sellableQuantity = 2;
    other.components[0].definition.slug = target.components[0].definition.slug;
    other.components[0].definition.requiredQuantity = 2;
    other.definition.components[0].slug = target.components[0].definition.slug;
    other.definition.components[0].requiredQuantity = 2;

    const reserved = reservePublishedSetListings(target, [{
      itemId: other.itemId ?? null,
      type: "sell",
      quantity: 1,
      visible: true,
      rank: null,
      charges: null,
      subtype: null,
      amberStars: null,
      cyanStars: null,
    }], [target, other]);

    expect(reserved.components[0].sellableQuantity).toBe(0);
    expect(setOpportunity(reserved).sellableCompleteSets).toBe(0);
  });

  it("sorts completion candidates by executable profit rather than the headline set premium", () => {
    const betterProfit = setRow("Better Profit", 1, 0, 100, 80);
    const higherPremium = setRow("Higher Premium", 1, 0, 120, 60);
    higherPremium.components[0].recommendation!.lowestAsk = 60;

    expect(setOpportunity(betterProfit).completionProfit).toBe(20);
    expect(setOpportunity(higherPremium).setPremiumValue).toBe(60);
    expect(setOpportunity(higherPremium).completionProfit).toBe(10);
    expect(filterAndSortOpportunitySets([higherPremium, betterProfit], [], "buy"))
      .toEqual([betterProfit, higherPremium]);
    expect(filterAndSortSets([higherPremium, betterProfit], "finish"))
      .toEqual([betterProfit, higherPremium]);
  });

  it("searches and shows the localized set name", () => {
    const localized = setRow("Nyx Prime Set", 1);
    localized.displayName = "Никс Прайм: Комплект";
    expect(filterAndSortSets([localized], "all", "никс")).toEqual([localized]);
    expect(filterAndSortSets([localized], "all", "Nyx")).toEqual([]);
  });

  it("shows owned relics that can drop a missing set part", () => {
    const row = setRow("Strun Prime Set", 1);
    const usefulRelic = relicRow(`${row.definition.setSlug}_a`);
    const irrelevantRelic = relicRow("other_prime_part", 25, 10);

    const support = setRelicSupport(row, [irrelevantRelic, usefulRelic]);

    expect(support.matches).toHaveLength(1);
    expect(support.ownedRelicCount).toBe(3);
    expect(support.coveredPartCount).toBe(1);
    expect(support.allMissingPartsCovered).toBe(true);
    expect(support.aggregateChancePercent).toBeCloseTo(27.1, 5);
    expect(filterAndSortOpportunitySets([row], [usefulRelic], "relics")).toEqual([row]);
  });

  it("calculates the probability of collecting every missing part, not merely one useful drop", () => {
    const row = setRow("Dual Reward Set", 2);
    const first = relicRow(row.definition.components[0].slug, 50, 1);
    const second = relicRow(row.definition.components[1].slug, 50, 1);

    const support = setRelicSupport(row, [first, second]);

    expect(support.atLeastOneUsefulChancePercent).toBeCloseTo(75, 5);
    expect(support.aggregateChancePercent).toBeCloseTo(25, 5);
    expect(support.expectedUsefulDrops).toBeCloseTo(1, 5);
  });

  it("caps expected useful drops by the actual number of missing copies", () => {
    const row = setRow("Twin Part Set", 1);
    row.definition.components[0].requiredQuantity = 2;
    row.components[0].definition.requiredQuantity = 2;
    row.components[0].ownedQuantity = 0;
    const relic = relicRow(row.definition.components[0].slug, 100, 10);

    const support = setRelicSupport(row, [relic]);

    expect(support.aggregateChancePercent).toBe(100);
    expect(support.expectedUsefulDrops).toBe(2);
    expect(support.matches[0].expectedUsefulDrops).toBe(2);
  });

  it("recommends radiant refinement when a rare reward completes the next set", () => {
    const set = setRow("Strun Prime Set", 1);
    const relic = pricedRelicRow("axi_s1_relic", "intact", `${set.definition.setSlug}_a`);

    const [recommendation] = rankRelicsToOpen([relic], [set]);

    expect(recommendation.recommendedRefinement).toBe("radiant");
    expect(recommendation.sourceRefinement).toBe("intact");
    expect(recommendation.traceCost).toBe(100);
    expect(recommendation.completionChancePercent).toBe(10);
    expect(recommendation.completionTargets.map((target) => target.displayName)).toEqual([set.displayName]);
    expect(recommendation.grossExpectedPlatinum).toBeCloseTo(6.5001, 4);
    expect(recommendation.relicOpportunityCost).toBe(2);
    expect(recommendation.traceOpportunityCost).toBe(2);
    expect(recommendation.expectedPlatinum).toBeCloseTo(4.5001, 4);
    expect(recommendation.squadExpectedPlatinum).toBeGreaterThan(recommendation.expectedPlatinum ?? 0);
  });

  it("does not spend traces that are absent from the supplied inventory balance", () => {
    const set = setRow("Harrow Prime Set", 1);
    const relic = pricedRelicRow("axi_h1_relic", "intact", `${set.definition.setSlug}_a`);

    const [recommendation] = rankRelicsToOpen([relic], [set], { availableTraces: 0 });

    expect(recommendation.recommendedRefinement).toBe("intact");
    expect(recommendation.traceCost).toBe(0);
  });

  it("does not rank a relic when even one reward has only low-confidence pricing", () => {
    const relic = pricedRelicRow("meso_l1_relic", "intact", "low_confidence_rare");
    relic.rewards[0].recommendation = recommendation(
      relic.rewards[0].definition.rewardSlug!,
      500,
      "low",
    );

    const [result] = rankRelicsToOpen([relic], []);

    expect(result.grossExpectedPlatinum).toBeNull();
    expect(result.expectedPlatinum).toBeNull();
    expect(result.priorityScore).toBe(0);
  });

  it("does not inflate relic value when reward probabilities are duplicated", () => {
    const relic = pricedRelicRow("meso_d1_relic", "intact", "duplicated_rare");
    relic.rewards.push({ ...relic.rewards[0] });

    const [result] = rankRelicsToOpen([relic], []);

    expect(result.grossExpectedPlatinum).toBeNull();
    expect(result.squadGrossExpectedPlatinum).toBeNull();
    expect(result.priorityScore).toBe(0);
  });

  it("keeps an intact relic intact when its common reward is the valuable one", () => {
    const relic = pricedRelicRow("lith_c1_relic", "intact", "unrelated_rare", 1);
    relic.rewards[0].recommendation = recommendation(relic.rewards[0].definition.rewardSlug!, 50);

    const [result] = rankRelicsToOpen([relic], []);

    expect(result.recommendedRefinement).toBe("intact");
    expect(result.traceCost).toBe(0);
  });

  it("does not ask for traces when the recommended refinement is already owned", () => {
    const set = setRow("Ivara Prime Set", 1);
    const relic = pricedRelicRow("neo_i1_relic", "radiant", `${set.definition.setSlug}_a`);

    const [recommendation] = rankRelicsToOpen([relic], [set]);

    expect(recommendation.recommendedRefinement).toBe("radiant");
    expect(recommendation.traceCost).toBe(0);
    expect(recommendation.sourceQuantity).toBe(2);
  });
});
