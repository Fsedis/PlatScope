export const WARFRAME_GAME_ID = 8954;
export const MAX_EXPORT_BYTES = 8 * 1024 * 1024;
export const MAX_INVENTORY_ROWS = 100_000;
export const MAX_JSON_DEPTH = 64;
export const MAX_JSON_NODES = 250_000;
export const MAX_ITEM_QUANTITY = 1_000_000;

const PRODUCER = "platscope-overwolf-companion";
const MAX_IDENTIFIER_LENGTH = 256;

export interface CategorySummary {
  name: string;
  rows: number;
  quantity: number;
}

export interface InventoryAnalysis {
  value: Record<string, unknown>;
  rowCount: number;
  distinctItemCount: number;
  totalQuantity: number;
  categories: CategorySummary[];
}

export interface CompanionEnvelope {
  schemaVersion: 1;
  producer: typeof PRODUCER;
  observedAt: string;
  gameId: typeof WARFRAME_GAME_ID;
  feature: "match_info";
  key: "inventory";
  complete: true;
  value: Record<string, unknown>;
}

export type SnapshotErrorCode =
  | "invalid_json"
  | "invalid_shape"
  | "payload_too_large"
  | "nesting_too_deep"
  | "node_limit"
  | "row_limit"
  | "invalid_item"
  | "items_missing";

export class SnapshotError extends Error {
  constructor(public readonly code: SnapshotErrorCode) {
    super(code);
    this.name = "SnapshotError";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function decodeJson(value: unknown): unknown {
  if (typeof value !== "string") return value;
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return value;
  }
}

function inventoryFromContainer(container: unknown): unknown | null {
  const decoded = decodeJson(container);
  if (!isRecord(decoded)) return null;

  const feature = decoded.feature;
  if (typeof feature === "string" && feature !== "match_info") return null;

  if (decoded.key === "inventory") {
    const category = decoded.category;
    if (category === undefined || category === "match_info" || category === "game_info") {
      return decoded.value ?? decoded.data ?? null;
    }
  }

  const matchInfo = decodeJson(decoded.match_info);
  if (isRecord(matchInfo) && "inventory" in matchInfo) {
    return matchInfo.inventory;
  }

  if (feature === "match_info" && "inventory" in decoded) {
    return decoded.inventory;
  }

  return null;
}

/** Извлекает только documented `match_info.inventory` из event/getInfo wrappers. */
export function extractInventoryValue(update: unknown): unknown | null {
  const decoded = decodeJson(update);
  const direct = inventoryFromContainer(decoded);
  if (direct !== null) return direct;
  if (!isRecord(decoded)) return null;

  const info = decodeJson(decoded.info);
  const nested = inventoryFromContainer(info);
  if (nested !== null) return nested;

  if (isRecord(info) && "info" in info) {
    return inventoryFromContainer(info.info);
  }
  return null;
}

function categoryFromPath(path: readonly string[]): string {
  for (let index = path.length - 1; index >= 0; index -= 1) {
    const part = path[index];
    if (part && part !== "Inventory") return part;
  }
  return "Inventory";
}

function parseQuantity(value: unknown): number {
  if (value === undefined) return 1;
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value <= 0 ||
    value > MAX_ITEM_QUANTITY
  ) {
    throw new SnapshotError("invalid_item");
  }
  return value;
}

/** Проверяет границы и строит summary, не сохраняя дополнительные персональные поля. */
export function analyzeInventoryValue(value: unknown): InventoryAnalysis {
  let decoded: unknown;
  if (typeof value === "string") {
    if (new TextEncoder().encode(value).byteLength > MAX_EXPORT_BYTES) {
      throw new SnapshotError("payload_too_large");
    }
    try {
      decoded = JSON.parse(value) as unknown;
    } catch {
      throw new SnapshotError("invalid_json");
    }
  } else {
    decoded = value;
  }

  if (!isRecord(decoded)) throw new SnapshotError("invalid_shape");

  const encoded = JSON.stringify(decoded);
  if (new TextEncoder().encode(encoded).byteLength > MAX_EXPORT_BYTES) {
    throw new SnapshotError("payload_too_large");
  }

  let nodes = 0;
  let rowCount = 0;
  let totalQuantity = 0;
  const distinctItems = new Set<string>();
  const categoryTotals = new Map<string, { rows: number; quantity: number }>();

  const visit = (node: unknown, depth: number, path: readonly string[]): void => {
    if (depth > MAX_JSON_DEPTH) throw new SnapshotError("nesting_too_deep");
    nodes += 1;
    if (nodes > MAX_JSON_NODES) throw new SnapshotError("node_limit");

    if (Array.isArray(node)) {
      for (const child of node) visit(child, depth + 1, path);
      return;
    }
    if (!isRecord(node)) return;

    if ("ItemType" in node) {
      if (
        typeof node.ItemType !== "string" ||
        node.ItemType.trim().length === 0 ||
        node.ItemType.length > MAX_IDENTIFIER_LENGTH
      ) {
        throw new SnapshotError("invalid_item");
      }
      const quantity = parseQuantity(node.ItemCount);
      rowCount += 1;
      if (rowCount > MAX_INVENTORY_ROWS) throw new SnapshotError("row_limit");
      totalQuantity += quantity;
      if (!Number.isSafeInteger(totalQuantity)) throw new SnapshotError("invalid_item");
      const rank = node.Rank ?? node.UpgradeLevel ?? "";
      distinctItems.add(`${node.ItemType.trim()}\u0000${String(rank)}`);
      const category = categoryFromPath(path);
      const totals = categoryTotals.get(category) ?? { rows: 0, quantity: 0 };
      totals.rows += 1;
      totals.quantity += quantity;
      categoryTotals.set(category, totals);
      return;
    }

    for (const [key, child] of Object.entries(node)) {
      visit(child, depth + 1, [...path, key]);
    }
  };

  visit(decoded, 0, []);
  if (rowCount === 0) throw new SnapshotError("items_missing");

  const categories = [...categoryTotals.entries()]
    .map(([name, totals]) => ({ name, ...totals }))
    .sort((left, right) => right.rows - left.rows || left.name.localeCompare(right.name));

  return {
    value: decoded,
    rowCount,
    distinctItemCount: distinctItems.size,
    totalQuantity,
    categories,
  };
}

export function createCompanionEnvelope(
  analysis: InventoryAnalysis,
  observedAt: Date,
): CompanionEnvelope {
  if (Number.isNaN(observedAt.getTime())) throw new SnapshotError("invalid_shape");
  return {
    schemaVersion: 1,
    producer: PRODUCER,
    observedAt: observedAt.toISOString(),
    gameId: WARFRAME_GAME_ID,
    feature: "match_info",
    key: "inventory",
    complete: true,
    value: analysis.value,
  };
}

export function serializeEnvelope(envelope: CompanionEnvelope): string {
  const serialized = `${JSON.stringify(envelope)}\n`;
  if (new TextEncoder().encode(serialized).byteLength > MAX_EXPORT_BYTES) {
    throw new SnapshotError("payload_too_large");
  }
  return serialized;
}

export function isAbsoluteJsonPath(path: string): boolean {
  const trimmed = path.trim();
  if (trimmed.length === 0 || trimmed.length > 1024 || /[\u0000-\u001f]/u.test(trimmed)) {
    return false;
  }
  const drivePath = /^[A-Za-z]:[\\/](?:[^<>:"|?*\r\n]+[\\/])*[^<>:"|?*\r\n]+\.json$/iu;
  const uncPath = /^\\\\[^\\/:*?"<>|\r\n]+\\[^\\/:*?"<>|\r\n]+(?:\\[^\\/:*?"<>|\r\n]+)*\.json$/iu;
  return drivePath.test(trimmed) || uncPath.test(trimmed);
}

export function isWarframeRunning(gameInfo: unknown): boolean {
  if (!isRecord(gameInfo) || gameInfo.isRunning !== true) return false;
  const ids = [gameInfo.id, gameInfo.classId];
  return ids.some(
    (value) =>
      typeof value === "number" &&
      (value === WARFRAME_GAME_ID || Math.floor(value / 10) === WARFRAME_GAME_ID),
  );
}

export function russianPlural(value: number, one: string, few: string, many: string): string {
  const absolute = Math.abs(value);
  const lastTwo = absolute % 100;
  if (lastTwo >= 11 && lastTwo <= 14) return many;
  const last = absolute % 10;
  if (last === 1) return one;
  if (last >= 2 && last <= 4) return few;
  return many;
}

export function englishPlural(value: number, one: string, many: string): string {
  return Math.abs(value) === 1 ? one : many;
}
