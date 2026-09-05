import type { ActivityOffer } from "./worldActivity";

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
