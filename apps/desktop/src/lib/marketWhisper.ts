import type { LiveOrderView, MarketVariantKey } from "./market";

/** Текст для буфера обмена, без отправки в игру или на сайт. */
export function marketWhisper(offer: LiveOrderView, englishName: string, key: MarketVariantKey | null): string | null {
  const name = englishName.trim();
  if (!name || /[\r\n]/.test(name) || !offer.userIngameName || !/^[\p{L}\p{N}._-]{1,100}$/u.test(offer.userIngameName)) return null;
  const variant: string[] = [];
  if (key?.rank !== null && key?.rank !== undefined) variant.push(`rank ${key.rank}`);
  if (key?.charges !== null && key?.charges !== undefined) variant.push(`${key.charges} charges`);
  if (key?.subtype) variant.push(key.subtype.replace(/[\r\n]/g, " "));
  if (key?.amberStars !== null && key?.amberStars !== undefined) variant.push(`${key.amberStars} amber stars`);
  if (key?.cyanStars !== null && key?.cyanStars !== undefined) variant.push(`${key.cyanStars} cyan stars`);
  return `/w ${offer.userIngameName} Hi! I want to ${offer.side === "sell" ? "buy" : "sell"}: ${name}${variant.length ? ` (${variant.join(", ")})` : ""}${offer.perTrade > 1 ? ` (${offer.perTrade} items)` : ""} for ${offer.platinum} platinum. (warframe.market)`;
}
