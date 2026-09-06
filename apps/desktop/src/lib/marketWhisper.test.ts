import { describe, expect, it } from "vitest";
import { marketWhisper } from "./marketWhisper";
import type { LiveOrderView, MarketVariantKey } from "./market";

const offer: LiveOrderView = {side:"sell",platinum:30,quantity:12,perTrade:3,userStatus:"in_game",userIngameName:"Some.Tenno"};
const key: MarketVariantKey = {slug:"some_arcane",platform:"pc",rank:0,charges:null,subtype:null,amberStars:null,cyanStars:null};
describe("сообщение участнику рынка", () => {
  it("сохраняет ранг0 и цену партии вместо всей доступной пачки", () => {
    expect(marketWhisper(offer,"Some Arcane",key)).toContain("buy: Some Arcane (rank 0) (3 items) for 30 platinum");
    expect(marketWhisper({...offer,side:"buy"},"Some Arcane",key)).toContain("want to sell:");
  });
  it("передаёт подтип, заряды и обе разновидности звёзд", () => {
    expect(marketWhisper(offer,"Ayatan",{...key,rank:null,charges:0,subtype:"regular",amberStars:0,cyanStars:2})).toContain("0 charges, regular, 0 amber stars, 2 cyan stars");
  });
  it("не составляет сообщение без английского названия или с управляющими символами в имени", () => {
    expect(marketWhisper(offer,"",key)).toBeNull();
    expect(marketWhisper({...offer,userIngameName:"Tenno\n/other"},"Item",key)).toBeNull();
  });
});
