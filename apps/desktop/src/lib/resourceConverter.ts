export type ResourceSource = "syndicate" | "nightwave" | "void_trader" | "steel_path";
export type ResourceCurrency = "standing" | "nightwave_cred" | "ducat" | "steel_essence";
export type ResourceRouteStatus = "ready" | "conditional" | "waiting" | "unavailable" | "needs_data";
export type ArcaneDecisionKind = "sell" | "dissolve" | "hold";

export interface ResourceConversionAction {
  vendorName: string;
  currency: ResourceCurrency;
  balance: number;
  cost: number;
  itemSlug: string;
  itemName: string;
  imageUrl?: string | null;
  quantity: number;
  unitPrice: number;
  estimatedPlatinum: number;
  includedInTotal: boolean;
}

export interface ResourceConversionRoute {
  source: ResourceSource;
  status: ResourceRouteStatus;
  reason: string;
  actions: ResourceConversionAction[];
  availableAt?: string | null;
  availableUntil?: string | null;
  location?: string | null;
}

export interface ArcaneConversionDecision {
  decision: ArcaneDecisionKind;
  slug: string;
  displayName: string;
  imageUrl?: string | null;
  rank: number;
  quantity: number;
  marketPriceEach?: number | null;
  vosforEach: number;
  vosforTotal: number;
  equivalentPlatinumEach: number;
  estimatedPlatinum: number;
}

export interface ArcaneConversionSummary {
  available: boolean;
  reason: string;
  bestPackName?: string | null;
  packExpectedPlatinum?: number | null;
  priceCoveragePercent: number;
  sell: ArcaneConversionDecision[];
  dissolve: ArcaneConversionDecision[];
  hold: ArcaneConversionDecision[];
  directSalePlatinum: number;
  dissolutionExpectedPlatinum: number;
}

export interface ResourceConverterView {
  fetchedAt: string;
  inventoryObservedAt: string;
  marketSourceDate?: string | null;
  confirmedPlatinum: number;
  expectedVosforPlatinum: number;
  routes: ResourceConversionRoute[];
  arcanes: ArcaneConversionSummary;
  unavailableSources: string[];
}

export function compactNumber(value: number, locale = "ru-RU"): string {
  return value.toLocaleString(locale, { maximumFractionDigits: 0 });
}

export function visibleArcaneDecisions(
  rows: ArcaneConversionDecision[],
  showAll: boolean,
  limit = 4,
): ArcaneConversionDecision[] {
  return showAll ? rows : rows.slice(0, limit);
}
