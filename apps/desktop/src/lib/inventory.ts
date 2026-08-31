import type { MarketVariantKey } from "./market";
import type { UiLocale } from "./i18n";

export type InventorySource =
  | "platscope_json"
  | "helper_import"
  | "overwolf_companion"
  | "test_fixture"
  | "read_only_scan";

export type InventoryResolution =
  | "resolved"
  | "unknown_item"
  | "ambiguous_item"
  | "exact_variant_unavailable";

export type VaultStatus = "available" | "vaulted" | "unknown";

export interface InventorySnapshotMetadata {
  source: InventorySource;
  observedAt: string;
  schemaVersion: number;
  itemCount: number;
  checksumSha256: string;
}

export interface InventorySummary {
  ownedQuantity: number;
  sellableQuantity: number;
  resolvedRows: number;
  attentionRows: number;
}

export type EquipmentKind =
  | "warframe"
  | "primary"
  | "secondary"
  | "melee"
  | "companion"
  | "companion_weapon"
  | "archwing"
  | "archgun"
  | "archmelee"
  | "necramech"
  | "amp"
  | "other";

export interface EquippedModPlacement {
  equipmentInstanceKey: string;
  equipmentGameId: string;
  equipmentDisplayName: string;
  equipmentImageUrl: string | null;
  equipmentKind: EquipmentKind;
  configIndex: number;
}

export interface InventoryViewItem {
  canonicalGameId: string;
  itemId: string | null;
  bulkTradable: boolean;
  displayName: string;
  imageUrl?: string | null;
  tags: string[];
  key: MarketVariantKey | null;
  rank: number | null;
  subtype: string | null;
  ownedQuantity: number;
  tradeableQuantity: number;
  untradeableQuantity: number;
  unknownQuantity: number;
  leveledQuantity: number;
  equippedQuantity: number;
  equippedPlacements: EquippedModPlacement[];
  sellableQuantity: number;
  resolution: InventoryResolution;
  vaultStatus: VaultStatus;
}

export interface InventoryView {
  metadata: InventorySnapshotMetadata;
  keepCopies: number;
  modUsageScanned: boolean;
  summary: InventorySummary;
  items: InventoryViewItem[];
}

export const INVENTORY_CATEGORIES = [
  "mod",
  "arcane_enhancement",
  "relic",
  "component",
  "weapon",
  "warframe",
  "misc",
] as const;
export type InventoryCategory = (typeof INVENTORY_CATEGORIES)[number];
export type InventoryCategoryFilter = "all" | InventoryCategory;

export function inventoryCategory(item: InventoryViewItem): InventoryCategory {
  const tags = new Set(item.tags);
  if (tags.has("arcane_enhancement")) return "arcane_enhancement";
  if (tags.has("mod")) return "mod";
  if (tags.has("relic")) return "relic";
  if (tags.has("component")) return "component";
  if (tags.has("weapon")) return "weapon";
  if (tags.has("warframe")) return "warframe";
  return "misc";
}

export function vaultStatusLabel(status: VaultStatus, locale: UiLocale = "ru"): string {
  return (locale === "en" ? {
    available: "Available",
    vaulted: "Vaulted",
    unknown: "Unknown",
  } : {
    available: "Доступен",
    vaulted: "В хранилище",
    unknown: "Неизвестно",
  })[status];
}

export function inventoryVariantLabel(item: InventoryViewItem, locale: UiLocale = "ru"): string {
  const dimensions = [
    item.rank === null ? null : `${locale === "en" ? "rank" : "ранг"} ${item.rank}`,
    item.subtype,
  ].filter(Boolean);
  return dimensions.length ? dimensions.join(" · ") : locale === "en" ? "base variant" : "базовый вариант";
}

export function resolutionLabel(resolution: InventoryResolution, locale: UiLocale = "ru"): string {
  switch (resolution) {
    case "resolved":
      return locale === "en" ? "Matched" : "Сопоставлен";
    case "unknown_item":
      return locale === "en" ? "Not in catalog" : "Нет в каталоге";
    case "ambiguous_item":
      return locale === "en" ? "Ambiguous ID" : "Неоднозначный ID";
    case "exact_variant_unavailable":
      return locale === "en" ? "Exact variant unavailable" : "Нет точного варианта";
  }
}

export function inventorySourceLabel(source: InventorySource, locale: UiLocale = "ru"): string {
  switch (source) {
    // Legacy-значения остаются отображаемыми для старых локальных снимков.
    // Текущий интерфейс новые снимки этих типов не создаёт.
    case "platscope_json":
      return "PlatScope JSON";
    case "helper_import":
      return locale === "en" ? "Imported file" : "Импортированный файл";
    case "overwolf_companion":
      return locale === "en" ? "Legacy Overwolf import" : "Старый импорт Overwolf";
    case "test_fixture":
      return locale === "en" ? "test data" : "тестовые данные";
    case "read_only_scan":
      return locale === "en" ? "Warframe scan" : "Сканирование Warframe";
  }
}
