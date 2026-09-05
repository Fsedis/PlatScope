import { describe, expect, it } from "vitest";
import { rotationEquipment, rotationRelics, rotationRewards } from "./primeResurgence";
import { makeWorldActivityMock } from "./worldActivityMock";

describe("Возрождение Прайм: предмет → реликвии → детали", () => {
  it("на настоящем ассортименте показывает все шесть реликвий обоим варфреймам и пять — Акболто", () => {
    const offers = makeWorldActivityMock("real").resurgenceOffers;
    const equipment = rotationEquipment(offers);
    const frames = equipment.filter(offer => offer.equipmentCategory === "warframe");
    expect(frames.map(offer => offer.displayName)).toEqual(["Банши Прайм", "Мираж Прайм"]);
    for (const frame of frames) {
      const relics = rotationRelics(offers, frame.gameRef);
      expect(relics).toHaveLength(6);
      for (const relic of relics) {
        const rewards = rotationRewards(relic, frames, frame.gameRef);
        expect(rewards).toHaveLength(1);
        expect(rewards[0].displayName).toContain(frame.displayName);
      }
    }
    const weapon = equipment.find(offer => offer.displayName === "Акболто Прайм")!;
    expect(rotationRelics(offers, weapon.gameRef)).toHaveLength(5);
    expect(rotationRelics(offers, weapon.gameRef).every(relic => relic.displayName !== "Реликвия Мезо E5")).toBe(true);
  });
  it("отделяет снаряжение от реликвий и не повторяет предметы", () => {
    const offers = makeWorldActivityMock(null).resurgenceOffers;
    expect(rotationEquipment([...offers, offers[0]])).toHaveLength(6);
    expect(rotationRelics(offers)).toHaveLength(6);
  });
  it("связывает награды по точному предмету, не по похожему названию", () => {
    const offers = makeWorldActivityMock(null).resurgenceOffers;
    const [banshee, mirage] = rotationEquipment(offers);
    const relics = rotationRelics(offers, banshee.gameRef);
    expect(relics).toHaveLength(3);
    for (const relic of relics) {
      expect(rotationRewards(relic, [banshee, mirage], banshee.gameRef)).toHaveLength(1);
      expect(rotationRewards(relic, [banshee, mirage], banshee.gameRef)[0].displayName).toContain("Банши");
    }
    expect(rotationRelics(offers, "unknown")).toEqual([]);
  });
  it("общая реликвия остаётся у обоих предметов, Форме не назначается сет", () => {
    const offers = makeWorldActivityMock(null).resurgenceOffers;
    const equipment = rotationEquipment(offers);
    const relic = rotationRelics(offers)[0];
    expect(rotationRewards(relic, equipment).some(reward => reward.displayName.includes("Форма"))).toBe(false);
    relic.rewards[0].equipmentRefs.push(equipment[1].gameRef);
    expect(rotationRelics(offers, equipment[0].gameRef)).toContain(relic);
    expect(rotationRelics(offers, equipment[1].gameRef)).toContain(relic);
  });
  it("не придумывает состав при отсутствующем справочнике", () => {
    const offers = makeWorldActivityMock("catalog").resurgenceOffers;
    expect(rotationEquipment(offers)).toEqual([]);
    expect(rotationRelics(offers)).toHaveLength(6);
    expect(offers.every(offer => offer.rewards.length === 0)).toBe(true);
  });
});
