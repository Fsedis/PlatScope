import { describe, expect, it } from "vitest";

import {
  coverageLabel,
  filterAndSortOpportunitySets,
  filterAndSortSets,
  formatPercent,
  formatRatio,
  refinementLabel,
  setModeLabel,
  setOpportunity,
  setRelicSupport,
  type RelicInsightRow,
  type SetInsightRow,
  vaultLabel,
} from "./insights";
import type { PriceConfidence, PriceRecommendation } from "./market";

function recommendation(
  slug: string,
  fairPrice: number | null,
  confidence: PriceConfidence = "medium",
): PriceRecommendation {
  return {
    key: { slug, platform: "pc", rank: null, subtype: null, amberStars: null, cyanStars: null },
    provider: "relics_run",
    sourceDate: "2026-08-27",
    fairPrice,
    listPrice: fairPrice,
    quickSell: null,
    lowestAsk: null,
    depthThree: null,
    depthPrice: null,
    closedVolume: 20,
    liveSellOrderCount: 0,
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
        recommendation: recommendation(`${slug}_a`, 30),
      },
      {
        itemId: `${slug}_b-id`,
        displayName: `${name} B`,
        definition: { slug: `${slug}_b`, gameRef: "/Lotus/B", requiredQuantity: 1, ducats: 45 },
        ownedQuantity: completeSets + (missing < 2 ? 1 : 0),
        recommendation: recommendation(`${slug}_b`, 50),
      },
    ],
  };
}

describe("insights presentation", () => {
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
    expect(opportunity.profitableToComplete).toBe(true);
  });

  it("separates profitable completion candidates from ready sets", () => {
    const easy = setRow("Easy Set", 1);
    const harder = setRow("Hard Set", 2);
    const ready = setRow("Ready Set", 0, 1);
    expect(filterAndSortSets([harder, ready, easy], "finish").map((row) => row.displayName))
      .toEqual(["Easy Set", "Hard Set"]);
    expect(filterAndSortSets([easy, ready], "ready")).toEqual([ready]);
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
});
