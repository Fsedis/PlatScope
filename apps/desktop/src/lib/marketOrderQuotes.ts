import type { AccountProfile } from "./account";
import type { LivePricingResult, MarketVariantKey, PriceRecommendation } from "./market";
import { recommendationIdentity } from "./tradeShift";

export interface OrderPriceSession {
  quotes: Map<string, LivePricingResult>;
  failed: Set<string>;
  revisions: Map<string, number>;
}

// Срок сетевого кеша ограничивает повторный запрос, а не приоритет уже проверенной цены.
const sessions = new Map<string, OrderPriceSession>();
export const orderPricingContext: { crossplay: boolean | null } = { crossplay: null };

export function orderPriceContext(profile: AccountProfile, crossplay: boolean | null): string {
  return JSON.stringify([profile.id, profile.platform, profile.crossplay, crossplay]);
}

export function orderPriceSession(profile: AccountProfile, crossplay: boolean): OrderPriceSession {
  const identity = orderPriceContext(profile, crossplay);
  let session = sessions.get(identity);
  if (!session) {
    session = { quotes: new Map(), failed: new Set(), revisions: new Map() };
    sessions.set(identity, session);
  }
  return session;
}

export function acceptOrderPrice(
  session: OrderPriceSession,
  key: MarketVariantKey,
  result: LivePricingResult | null,
  expectedRevision = orderPriceRevision(session, key),
): boolean {
  const identity = recommendationIdentity(key);
  const previous = session.quotes.get(identity);
  const fetchedAt = result ? Date.parse(result.fetchedAt) : NaN;
  if (previous && Number.isFinite(fetchedAt) && Date.parse(previous.fetchedAt) > fetchedAt) return false;
  if (!result || result.quoteState === "stale_cache"
    || recommendationIdentity(result.recommendation.key) !== identity
    || !Number.isFinite(fetchedAt)) {
    failOrderPrice(session, key, expectedRevision);
    return false;
  }
  // Один и тот же livebook backend может пересчитать по новому снимку. Сохраняем
  // первоначальную проверенную оценку до получения действительно новой книги.
  if (!previous || Date.parse(previous.fetchedAt) < fetchedAt) session.quotes.set(identity, result);
  session.failed.delete(identity);
  session.revisions.set(identity, orderPriceRevision(session, key) + 1);
  return true;
}

export function orderPriceRevision(session: OrderPriceSession, key: MarketVariantKey): number {
  return session.revisions.get(recommendationIdentity(key)) ?? 0;
}

export function failOrderPrice(session: OrderPriceSession, key: MarketVariantKey, expectedRevision: number): boolean {
  if (orderPriceRevision(session, key) !== expectedRevision) return false;
  session.failed.add(recommendationIdentity(key));
  return true;
}

export function checkedOrderRecommendation(
  result: LivePricingResult,
  profile: AccountProfile | null,
): PriceRecommendation {
  const normalize = (value: string | null | undefined) => value?.trim().toLowerCase();
  const sellers = result.orders.filter(offer => offer.side === "sell" && offer.userStatus === "in_game"
    && Number.isFinite(offer.platinum) && offer.platinum > 0
    && Number.isInteger(offer.perTrade) && offer.perTrade > 0
    && Number.isInteger(offer.quantity) && offer.quantity >= offer.perTrade
    && !(profile?.slug && normalize(offer.userSlug) === normalize(profile.slug))
    && !(profile?.ingameName && normalize(offer.userIngameName) === normalize(profile.ingameName)));
  const price = result.recommendation.listPrice;
  // При малом числе продавцов и срабатывании защиты backend возвращает цену снимка.
  // Она не подтверждает текущую цену объявления даже после успешного live-запроса.
  const usable = result.quoteState !== "stale_cache" && sellers.length >= 3
    && !result.recommendation.reasons.some(reason => reason.code === "thin_market_protection")
    && price != null && Number.isFinite(price) && price > 0;
  return usable ? result.recommendation : { ...result.recommendation, listPrice: null, fairPrice: null };
}

export function mergeOrderPrices(
  saved: ReadonlyMap<string, PriceRecommendation | null>,
  quotes: ReadonlyMap<string, LivePricingResult>,
  profile: AccountProfile | null,
): Map<string, PriceRecommendation | null> {
  const merged = new Map(saved);
  for (const [identity, result] of quotes) merged.set(identity, checkedOrderRecommendation(result, profile));
  return merged;
}
