import type { OverwolfApi, OverwolfEvent, OverwolfResult } from "./overwolf";

class MockEvent<T> implements OverwolfEvent<T> {
  private readonly listeners = new Set<(event: T) => void>();

  addListener(listener: (event: T) => void): void {
    this.listeners.add(listener);
  }

  removeListener(listener: (event: T) => void): void {
    this.listeners.delete(listener);
  }

  emit(event: T): void {
    for (const listener of this.listeners) listener(event);
  }
}

export interface MockOverwolfApi extends OverwolfApi {
  readonly writtenFiles: Map<string, string>;
}

const demoInventory = {
  Inventory: {
    MiscItems: [
      { ItemType: "/Lotus/Types/Items/MiscItems/OrokinCell", ItemCount: 18 },
      { ItemType: "/Lotus/Types/Game/Projections/AxiN10Bronze", ItemCount: 4 },
    ],
    Recipes: [
      { ItemType: "/Lotus/Types/Recipes/WarframeRecipes/NyxPrimeBlueprint", ItemCount: 2 },
    ],
    Upgrades: [
      { ItemType: "/Lotus/Upgrades/Mods/Warframe/Expert/AvatarPowerMaxModExpert", ItemCount: 1, Rank: 10 },
    ],
  },
};

export function createMockOverwolfApi(): MockOverwolfApi {
  const infoUpdates = new MockEvent<unknown>();
  const errors = new MockEvent<unknown>();
  const writtenFiles = new Map<string, string>();
  return {
    writtenFiles,
    games: {
      getRunningGameInfo(callback): void {
        callback({ success: true, isRunning: true, id: 8954, classId: 8954 });
      },
      events: {
        setRequiredFeatures(features, callback): void {
          const result: OverwolfResult & { supportedFeatures?: string[] } = {
            success: features.length === 1 && features[0] === "match_info",
            supportedFeatures: ["match_info"],
          };
          callback(result);
        },
        getInfo(callback): void {
          callback({ success: true, info: { match_info: { inventory: demoInventory } } });
        },
        onInfoUpdates2: infoUpdates,
        onError: errors,
      },
    },
    io: {
      writeFileContents(path, content, encoding, triggerUacIfRequired, callback): void {
        if (encoding !== "UTF8" || triggerUacIfRequired) {
          callback({ success: false, error: "Unexpected write options" });
          return;
        }
        writtenFiles.set(path, content);
        callback({ success: true });
      },
    },
  };
}
