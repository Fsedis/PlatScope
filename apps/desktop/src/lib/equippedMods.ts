import type {
  EquipmentKind,
  EquippedModPlacement,
  InventoryViewItem,
} from "./inventory";

export interface EquippedModLocation {
  instanceKey: string;
  gameId: string;
  displayName: string;
  imageUrl: string | null;
  kind: EquipmentKind;
  configIndexes: number[];
}

export interface EquippedModEntry {
  identity: string;
  canonicalGameId: string;
  displayName: string;
  imageUrl: string | null;
  rank: number | null;
  equippedQuantity: number;
  sellableQuantity: number;
  equipmentCount: number;
  configCount: number;
  kinds: EquipmentKind[];
  locations: EquippedModLocation[];
}

export interface EquippedModsSummary {
  modVariants: number;
  modCopies: number;
  equipmentCount: number;
  configCount: number;
}

export function buildEquippedModEntries(
  items: InventoryViewItem[],
): EquippedModEntry[] {
  return items
    .filter((item) => item.equippedQuantity > 0 && item.equippedPlacements.length > 0)
    .map((item) => {
      const locations = groupPlacements(item.equippedPlacements);
      return {
        identity: modIdentity(item),
        canonicalGameId: item.canonicalGameId,
        displayName: item.displayName,
        imageUrl: item.imageUrl ?? null,
        rank: item.rank,
        equippedQuantity: item.equippedQuantity,
        sellableQuantity: item.sellableQuantity,
        equipmentCount: locations.length,
        configCount: locations.reduce(
          (sum, location) => sum + location.configIndexes.length,
          0,
        ),
        kinds: [...new Set(locations.map((location) => location.kind))],
        locations,
      };
    })
    .sort((left, right) =>
      left.displayName.localeCompare(right.displayName, "ru")
      || (right.rank ?? -1) - (left.rank ?? -1),
    );
}

export function filterEquippedModEntries(
  entries: EquippedModEntry[],
  query = "",
  kind: EquipmentKind | "all" = "all",
): EquippedModEntry[] {
  const normalizedQuery = query.trim().toLocaleLowerCase("ru");
  return entries.filter((entry) => {
    if (kind !== "all" && !entry.kinds.includes(kind)) return false;
    if (!normalizedQuery) return true;
    return entry.displayName.toLocaleLowerCase("ru").includes(normalizedQuery)
      || entry.canonicalGameId.toLocaleLowerCase("ru").includes(normalizedQuery);
  });
}

export function summarizeEquippedMods(
  entries: EquippedModEntry[],
): EquippedModsSummary {
  const equipmentKeys = new Set<string>();
  for (const entry of entries) {
    for (const location of entry.locations) equipmentKeys.add(location.instanceKey);
  }
  return {
    modVariants: entries.length,
    modCopies: entries.reduce((sum, entry) => sum + entry.equippedQuantity, 0),
    equipmentCount: equipmentKeys.size,
    configCount: entries.reduce((sum, entry) => sum + entry.configCount, 0),
  };
}

export function configLabel(index: number): string {
  return index >= 0 && index < 26 ? String.fromCharCode(65 + index) : String(index + 1);
}

function modIdentity(item: InventoryViewItem): string {
  return [
    item.canonicalGameId,
    item.rank ?? "base",
    item.subtype ?? "",
    item.key?.slug ?? item.itemId ?? "",
  ].join(":");
}

function groupPlacements(
  placements: EquippedModPlacement[],
): EquippedModLocation[] {
  const locations = new Map<string, {
    location: Omit<EquippedModLocation, "configIndexes">;
    configIndexes: Set<number>;
  }>();

  for (const placement of placements) {
    const holder = locations.get(placement.equipmentInstanceKey) ?? {
      location: equipmentFromPlacement(placement),
      configIndexes: new Set<number>(),
    };
    holder.configIndexes.add(placement.configIndex);
    locations.set(placement.equipmentInstanceKey, holder);
  }

  return [...locations.values()]
    .map(({ location, configIndexes }) => ({
      ...location,
      configIndexes: [...configIndexes].sort((left, right) => left - right),
    }))
    .sort((left, right) =>
      left.displayName.localeCompare(right.displayName, "ru"),
    );
}

function equipmentFromPlacement(
  placement: EquippedModPlacement,
): Omit<EquippedModLocation, "configIndexes"> {
  return {
    instanceKey: placement.equipmentInstanceKey,
    gameId: placement.equipmentGameId,
    displayName: placement.equipmentDisplayName,
    imageUrl: placement.equipmentImageUrl,
    kind: placement.equipmentKind,
  };
}
