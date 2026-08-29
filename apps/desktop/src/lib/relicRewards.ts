import type { MarketSearchRow } from "./market";

export interface RewardSetCompletion {
  setName: string;
  setPrice: number | null;
  incrementalValue: number | null;
}

export interface RewardSetPart {
  name: string;
  imageUrl: string | null;
  ownedQuantity: number;
  requiredQuantity: number;
  isReward: boolean;
}

export interface RewardSetOverview {
  setName: string;
  setPrice: number | null;
  readyComponents: number | null;
  totalComponents: number;
  parts: RewardSetPart[];
}

export interface RelicRewardChoice {
  slot: number;
  rawText: string;
  confidence: number;
  itemId: string | null;
  slug: string | null;
  displayName: string | null;
  market: MarketSearchRow | null;
  ducats: number | null;
  ownedQuantity: number | null;
  set: RewardSetOverview | null;
  completesSet: RewardSetCompletion | null;
  choiceValue: number | null;
  recommended: boolean;
}

export function ownedSetParts(choice: RelicRewardChoice): string {
  if (!choice.set || choice.set.readyComponents === null) return "Инвентарь не загружен";
  const ownedParts = choice.set.parts.filter((part) => part.ownedQuantity > 0);
  if (!ownedParts.length) return "Собранных частей пока нет";
  return ownedParts
    .map((part) => part.requiredQuantity > 1
      ? `${part.name} ${Math.min(part.ownedQuantity, part.requiredQuantity)}/${part.requiredQuantity}`
      : part.name)
    .join(" · ");
}

export interface RelicRewardScanView {
  status: string;
  message: string | null;
  recognizedCount: number;
  scanDurationMs: number;
  captureWidth: number | null;
  captureHeight: number | null;
  overlayScale: number;
  theme: string | null;
  rewards: RelicRewardChoice[];
}

export function rewardPrice(choice: RelicRewardChoice): number | null {
  return choice.market?.recommendation.fairPrice
    ?? choice.market?.recommendation.listPrice
    ?? choice.market?.recommendation.quickSell
    ?? null;
}

export function confidencePercent(confidence: number): number {
  return Math.round(Math.max(0, Math.min(1, confidence)) * 100);
}

export function overlayContentScale(scale: number, devicePixelRatio = 1): number {
  const safeScale = Number.isFinite(scale) ? scale : 1;
  const safePixelRatio = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0
    ? devicePixelRatio
    : 1;
  return Math.max(0.1, safeScale / safePixelRatio);
}
