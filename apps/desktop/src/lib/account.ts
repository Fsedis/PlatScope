import type { InventoryViewItem } from "./inventory";
import type { MarketItemKind, MarketSearchRow } from "./market";
import type { UiLocale } from "./i18n";

export type AccountOrderType = "buy" | "sell";

export interface AccountProfile {
  id: string;
  ingameName: string;
  slug: string;
  platform: string;
  crossplay: boolean;
  verification: boolean;
}

export interface AccountOrder {
  id: string;
  itemId: string | null;
  type: AccountOrderType;
  platinum: number;
  quantity: number;
  perTrade: number | null;
  rank: number | null;
  charges: number | null;
  subtype: string | null;
  amberStars: number | null;
  cyanStars: number | null;
  visible: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface AccountView {
  connected: boolean;
  profile: AccountProfile | null;
  orders: AccountOrder[];
  orderItems?: Record<string, AccountOrderItem>;
}

export interface AccountOrderItem {
  slug: string;
  displayName: string;
  displayNameEn: string;
  imageUrl: string | null;
  itemKind: MarketItemKind;
  setComponents?: AccountSetComponent[];
}

export interface AccountSetComponent {
  slug: string;
  requiredQuantity: number;
  displayName: string;
  displayNameEn: string;
}

export function orderEnglishName(item: AccountOrderItem | undefined): string | null {
  const localized = item?.displayName.trim() ?? "";
  const english = item?.displayNameEn.trim() ?? "";
  return english && english !== localized ? english : null;
}

export interface CreateListingInput {
  itemId: string;
  type: AccountOrderType;
  platinum: number;
  quantity: number;
  visible: boolean;
  perTrade: number | null;
  rank: number | null;
  charges: number | null;
  subtype: string | null;
  amberStars: number | null;
  cyanStars: number | null;
}

export interface UpdateListingInput {
  platinum: number | null;
  quantity: number | null;
  visible: boolean | null;
  perTrade: number | null;
  rank: number | null;
  charges: number | null;
  subtype: string | null;
  amberStars: number | null;
  cyanStars: number | null;
}

export function createListingInput(
  row: MarketSearchRow,
  platinum: number,
  quantity: number,
  visible: boolean,
  perTrade: number | null,
): CreateListingInput {
  return {
    itemId: row.itemId,
    type: "sell",
    platinum,
    quantity,
    visible,
    perTrade,
    rank: row.recommendation.key.rank,
    charges: row.recommendation.key.charges,
    subtype: row.recommendation.key.subtype,
    amberStars: row.recommendation.key.amberStars,
    cyanStars: row.recommendation.key.cyanStars,
  };
}

export function createListingInputFromInventory(
  item: InventoryViewItem,
  platinum: number,
  quantity: number,
  visible: boolean,
  perTrade: number | null,
): CreateListingInput | null {
  if (!item.itemId || !item.key) return null;
  return {
    itemId: item.itemId,
    type: "sell",
    platinum,
    quantity,
    visible,
    perTrade: item.bulkTradable ? perTrade : null,
    rank: item.key.rank,
    charges: item.key.charges,
    subtype: item.key.subtype,
    amberStars: item.key.amberStars,
    cyanStars: item.key.cyanStars,
  };
}

export function accountActionErrorMessage(
  reason: string,
  locale: UiLocale = "ru",
): string {
  const normalized = reason.toLowerCase();
  if (normalized.includes("pertrade") || normalized.includes("per_trade")) {
    return locale === "en"
      ? "Set how many items are sold per trade. Use a value from 1 to 6 that divides the total quantity evenly."
      : "Укажите предметов за одну сделку: от 1 до 6, без остатка от общего количества.";
  }
  if (normalized.includes("already") || normalized.includes("duplicate")) {
    return locale === "en"
      ? "An order for this exact item variant already exists on WFM. Refresh your orders and edit the existing one."
      : "На Warframe Market уже есть ордер для этого варианта. Обновите список и измените существующий ордер.";
  }
  if (normalized.includes("authorization") || normalized.includes("unauthorized")) {
    return locale === "en"
      ? "The WFM session has expired. Reconnect the account and try again."
      : "Сессия Warframe Market истекла. Подключите аккаунт заново и повторите действие.";
  }
  if (normalized.includes("rate limit")) {
    return locale === "en"
      ? "WFM is temporarily limiting requests. Wait a moment and try again."
      : "Warframe Market временно ограничил запросы. Подождите немного и повторите действие.";
  }
  if (normalized.includes("зарезерв") || normalized.includes("недостаточно доступ")
    || normalized.includes("available quantity") || normalized.includes("cannot sell")) {
    return locale === "en"
      ? "The order exceeds the unreserved quantity in your inventory. Check existing orders and protected copies."
      : "Ордер превышает свободное количество в инвентаре. Проверьте существующие ордера и оставляемые копии.";
  }
  if (normalized.includes("точный вариант") || normalized.includes("exact variant")
    || normalized.includes("ранг") || normalized.includes("rank")
    || normalized.includes("заряд") || normalized.includes("charges")) {
    return locale === "en"
      ? "This exact rank or charged variant is not available for sale. Refresh the inventory and select the matching copy."
      : "Точного варианта с таким рангом или зарядами нет для продажи. Обновите инвентарь и выберите подходящую копию.";
  }
  if (normalized.includes("400") || normalized.includes("bad request") || normalized.includes("validation")) {
    return locale === "en"
      ? "WFM rejected the order parameters. Refresh market data, check the exact variant and quantity, then try again."
      : "Warframe Market не принял ордер. Обновите цены и проверьте вариант, цену и количество.";
  }
  return locale === "en"
    ? "WFM did not apply the action. Refresh your orders and try again."
    : "Warframe Market не применил действие. Обновите ордера и повторите попытку.";
}

export function matchingSellOrder(
  item: InventoryViewItem,
  account: AccountView | null,
): AccountOrder | null {
  if (!item.itemId || !item.key || !account?.connected) return null;
  return account.orders.find((order) =>
    order.type === "sell" &&
    order.itemId === item.itemId &&
    order.rank === item.key?.rank &&
    order.charges === item.key?.charges &&
    order.subtype === item.key?.subtype &&
    order.amberStars === item.key?.amberStars &&
    order.cyanStars === item.key?.cyanStars
  ) ?? null;
}

export function validateListingNumbers(
  platinum: number,
  quantity: number,
  perTrade: number | null,
  locale: UiLocale = "ru",
  maxQuantity: number | null = null,
): string | null {
  if (!Number.isInteger(platinum) || platinum < 1 || platinum > 900_000) {
    return locale === "en" ? "Enter a whole-number price from 1 to 900,000 platinum." : "Укажите цену целым числом от 1 до 900 000 платины.";
  }
  if (!Number.isInteger(quantity) || quantity < 1 || quantity > 9_999) {
    return locale === "en" ? "Enter a whole-number quantity from 1 to 9,999." : "Укажите количество целым числом от 1 до 9 999.";
  }
  if (maxQuantity !== null && quantity > maxQuantity) {
    return locale === "en"
      ? `Only ${maxQuantity} confirmed copies are available for sale.`
      : `Для продажи доступно только ${maxQuantity} подтверждённых копий.`;
  }
  if (
    perTrade !== null &&
    (!Number.isInteger(perTrade) || perTrade < 1 || perTrade > 6 || quantity % perTrade !== 0)
  ) {
    return locale === "en" ? "Per-trade quantity must be from 1 to 6 and divide the total quantity evenly." : "Количество за сделку должно быть от 1 до 6 и делить общее количество без остатка.";
  }
  return null;
}

export function orderTypeLabel(value: AccountOrderType, locale: UiLocale = "ru"): string {
  return value === "sell" ? locale === "en" ? "Sell" : "Продажа" : locale === "en" ? "Buy" : "Покупка";
}

export function visibilityLabel(value: boolean, locale: UiLocale = "ru"): string {
  return value ? locale === "en" ? "Published" : "Опубликован" : locale === "en" ? "Hidden" : "Скрыт";
}
