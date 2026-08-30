import type {
  EquipmentKind,
  EquippedModPlacement,
  InventoryViewItem,
} from "./inventory";

export interface EquippedModEntry {
  identity: string;
  displayName: string;
  imageUrl: string | null;
  rank: number | null;
  freeQuantity: number;
}

export interface EquipmentConfigGroup {
  index: number;
  mods: EquippedModEntry[];
}

export interface EquippedEquipmentGroup {
  instanceKey: string;
  gameId: string;
  displayName: string;
  imageUrl: string | null;
  kind: EquipmentKind;
  configs: EquipmentConfigGroup[];
}

export interface EquippedModsSummary {
  modCopies: number;
  equipmentCount: number;
  configCount: number;
}

export function buildEquippedEquipmentGroups(
  items: InventoryViewItem[],
  query = "",
  kind: EquipmentKind | "all" = "all",
): EquippedEquipmentGroup[] {
  const groups = new Map<string, {
    group: Omit<EquippedEquipmentGroup, "configs">;
    configs: Map<number, Map<string, EquippedModEntry>>;
  }>();

  for (const item of items) {
    const identity = `${item.canonicalGameId}:${item.rank ?? "base"}`;
    for (const placement of item.equippedPlacements) {
      const holder = groups.get(placement.equipmentInstanceKey) ?? {
        group: equipmentFromPlacement(placement),
        configs: new Map<number, Map<string, EquippedModEntry>>(),
      };
      const config = holder.configs.get(placement.configIndex) ?? new Map();
      config.set(identity, {
        identity,
        displayName: item.displayName,
        imageUrl: item.imageUrl ?? null,
        rank: item.rank,
        freeQuantity: item.sellableQuantity,
      });
      holder.configs.set(placement.configIndex, config);
      groups.set(placement.equipmentInstanceKey, holder);
    }
  }

  const normalizedQuery = query.trim().toLocaleLowerCase("ru");
  return [...groups.values()]
    .map(({ group, configs }) => ({
      ...group,
      configs: [...configs.entries()]
        .map(([index, mods]) => ({
          index,
          mods: [...mods.values()].sort((left, right) =>
            left.displayName.localeCompare(right.displayName, "ru"),
          ),
        }))
        .sort((left, right) => left.index - right.index),
    }))
    .filter((group) => {
      if (kind !== "all" && group.kind !== kind) return false;
      if (!normalizedQuery) return true;
      return group.displayName.toLocaleLowerCase("ru").includes(normalizedQuery)
        || group.gameId.toLocaleLowerCase("ru").includes(normalizedQuery)
        || group.configs.some((config) => config.mods.some((mod) =>
          mod.displayName.toLocaleLowerCase("ru").includes(normalizedQuery),
        ));
    })
    .sort((left, right) =>
      left.displayName.localeCompare(right.displayName, "ru"),
    );
}

export function summarizeEquippedMods(
  items: InventoryViewItem[],
  groups: EquippedEquipmentGroup[],
): EquippedModsSummary {
  return {
    modCopies: items.reduce((sum, item) => sum + item.equippedQuantity, 0),
    equipmentCount: groups.length,
    configCount: groups.reduce((sum, group) => sum + group.configs.length, 0),
  };
}

export function configLabel(index: number): string {
  return index >= 0 && index < 26 ? String.fromCharCode(65 + index) : String(index + 1);
}

function equipmentFromPlacement(
  placement: EquippedModPlacement,
): Omit<EquippedEquipmentGroup, "configs"> {
  return {
    instanceKey: placement.equipmentInstanceKey,
    gameId: placement.equipmentGameId,
    displayName: placement.equipmentDisplayName,
    imageUrl: placement.equipmentImageUrl,
    kind: placement.equipmentKind,
  };
}
