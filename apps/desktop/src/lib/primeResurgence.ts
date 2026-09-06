import type { ActivityOffer } from "./worldActivity";
import type { InventoryView } from "./inventory";

/** Все уровни улучшения объединяются по точному идентификатору реликвии. */
export function rotationRelicOwned(relic: ActivityOffer, inventory: InventoryView | null): number | null {
  if (!inventory || !relic.relicSlug) return null;
  return inventory.items.filter(item => item.key?.slug === relic.relicSlug)
    .reduce((total, item) => total + item.ownedQuantity, 0);
}

export function rotationEquipment(offers: ActivityOffer[]): ActivityOffer[] {
  return offers.filter((offer, index) => offer.kind === "equipment"
    && offers.findIndex(other => other.gameRef === offer.gameRef) === index);
}

export function rotationRelics(offers: ActivityOffer[], equipmentRef = ""): ActivityOffer[] {
  return offers.filter((offer, index) => offer.kind === "relic"
    && offers.findIndex(other => other.gameRef === offer.gameRef) === index
    && (!equipmentRef || offer.rewards.some(reward => reward.equipmentRefs.includes(equipmentRef))));
}

export function rotationRewards(relic: ActivityOffer, equipment: ActivityOffer[], selectedRef = "") {
  const refs = new Set(selectedRef ? [selectedRef] : equipment.map(offer => offer.gameRef));
  return relic.rewards.filter(reward => reward.equipmentRefs.some(ref => refs.has(ref)));
}
