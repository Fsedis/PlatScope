import { mockIPC } from "@tauri-apps/api/mocks";

import type {
  AccountOrder,
  AccountView,
  CreateListingInput,
  UpdateListingInput,
} from "./account";
import type { FoundationStatus, MarketRefreshOutcome } from "./foundation";
import { DEFAULT_APP_SETTINGS, type AppSettings } from "./i18n";
import type { DiagnosticsStatus } from "./diagnostics";
import type { MarketHistoryView } from "./history";
import type { InventoryView, InventoryViewItem } from "./inventory";
import type {
  GameMetadataRefreshOutcome,
  InsightsView,
  RelicRewardInsight,
  SetComponentInsight,
} from "./insights";
import type {
  MarketItemKind,
  LivePricingResult,
  MarketSearchResult,
  MarketSearchRow,
  PriceConfidence,
} from "./market";
import type { LiveSellNowResult, SellNowRow, SellNowView } from "./sellNow";

const sourceDate = "2026-08-26";
const snapshot = {
  provider: "relics_run" as const,
  sourceDate,
  fetchedAt: "2026-08-27T05:00:00Z",
  promotedAt: "2026-08-27T05:00:03Z",
  itemCount: 3840,
  recordCount: 13222,
  checksumSha256: "demo",
};

let inventory: InventoryView = {
  metadata: {
    source: "test_fixture",
    observedAt: "2026-08-27T08:30:00Z",
    schemaVersion: 1,
    itemCount: 2,
    checksumSha256: "inventory-demo",
  },
  keepCopies: 1,
  summary: {
    ownedQuantity: 22,
    sellableQuantity: 19,
    resolvedRows: 3,
    attentionRows: 1,
  },
  items: [
    {
      canonicalGameId: "nyx_prime_set",
      itemId: "demo-nyx_prime_set",
      bulkTradable: false,
      imageUrl: "https://warframe.market/static/assets/items/images/en/thumbs/nyx_prime_set.fd41c04c9e9bcc7e0e6963914f68f880.128x128.png",
      displayName: "Никс Прайм: комплект",
      tags: ["prime", "set", "warframe"],
      key: {
        slug: "nyx_prime_set",
        platform: "pc",
        rank: null,
        subtype: null,
        amberStars: null,
        cyanStars: null,
      },
      rank: null,
      subtype: null,
      ownedQuantity: 3,
      tradeableQuantity: 3,
      untradeableQuantity: 0,
      unknownQuantity: 0,
      leveledQuantity: 0,
      sellableQuantity: 2,
      resolution: "resolved",
      vaultStatus: "vaulted",
      closedMedian48h: 59,
      hasReliablePrice: true,
    },
    {
      canonicalGameId: "primed_flow",
      itemId: "demo-primed_flow",
      bulkTradable: false,
      imageUrl: "https://warframe.market/static/assets/items/images/en/thumbs/primed_flow.f65c5889e6d464ceea67c6c0aae9faa0.128x128.png",
      displayName: "Поток Прайм",
      tags: ["mod", "prime"],
      key: {
        slug: "primed_flow",
        platform: "pc",
        rank: 10,
        subtype: null,
        amberStars: null,
        cyanStars: null,
      },
      rank: 10,
      subtype: null,
      ownedQuantity: 3,
      tradeableQuantity: 3,
      untradeableQuantity: 0,
      unknownQuantity: 0,
      leveledQuantity: 3,
      sellableQuantity: 2,
      resolution: "resolved",
      vaultStatus: "unknown",
      closedMedian48h: 42,
      hasReliablePrice: true,
    },
    {
      canonicalGameId: "primary_deadhead",
      itemId: "demo-primary_deadhead",
      bulkTradable: true,
      displayName: "Primary Deadhead",
      tags: ["arcane_enhancement", "rare"],
      key: {
        slug: "primary_deadhead",
        platform: "pc",
        rank: 0,
        subtype: null,
        amberStars: null,
        cyanStars: null,
      },
      rank: 0,
      subtype: null,
      ownedQuantity: 16,
      tradeableQuantity: 16,
      untradeableQuantity: 0,
      unknownQuantity: 0,
      leveledQuantity: 0,
      sellableQuantity: 15,
      resolution: "resolved",
      vaultStatus: "unknown",
      closedMedian48h: 2,
      hasReliablePrice: true,
    },
  ],
};

let account: AccountView = { connected: false, profile: null, orders: [] };
let nextAccountOrder = 2;
let nextEventListener = 1;
let appSettings: AppSettings = { ...DEFAULT_APP_SETTINGS };

const englishNames: Record<string, string> = {
  nyx_prime_set: "Nyx Prime Set",
  nyx_prime_blueprint: "Nyx Prime Blueprint",
  nyx_prime_neuroptics: "Nyx Prime Neuroptics",
  nyx_prime_chassis: "Nyx Prime Chassis",
  nyx_prime_systems: "Nyx Prime Systems",
  axi_n10_relic: "Axi N10 Relic",
  primed_flow: "Primed Flow",
  primary_deadhead: "Primary Deadhead",
  ayatan_anasa_sculpture: "Ayatan Anasa Sculpture",
  secura_dual_cestra: "Secura Dual Cestra",
};

const masteryRequirements: Record<string, number> = {
  nyx_prime_set: 0,
  secura_dual_cestra: 12,
};

const rows: MarketSearchRow[] = [
  makeRow("Никс Прайм: комплект", "nyx_prime_set", 87, 14, "medium"),
  makeRow("Никс Прайм: нейрооптика", "nyx_prime_neuroptics", 24, 7, "medium"),
  makeRow("Никс Прайм: каркас", "nyx_prime_chassis", 18, 3, "medium"),
  makeRow("Никс Прайм: система", "nyx_prime_systems", null, 1, "unknown"),
  makeRow("Акси N10 реликвия", "axi_n10_relic", 8, 36, "medium", "relic", "radiant"),
  makeRow("Поток Прайм", "primed_flow", 73, 52, "medium", "standard", null, 10),
  makeRow("Primary Deadhead", "primary_deadhead", 2, 5, "medium", "standard", null, 0),
  makeRow("Скульптура Аятан Анаса", "ayatan_anasa_sculpture", 8, 401, "high"),
  makeRow("Секура Двойные Цестры", "secura_dual_cestra", 30, 9, "medium"),
];

function localizedName(slug: string, fallback: string): string {
  return appSettings.language === "english" ? englishNames[slug] ?? fallback : fallback;
}

function localizeMarketRow(row: MarketSearchRow): MarketSearchRow {
  const scoped = marketRowForPlatform(row, appSettings.platform);
  return { ...scoped, displayName: localizedName(scoped.recommendation.key.slug, scoped.displayName) };
}

function marketRowForPlatform(
  row: MarketSearchRow,
  platform: AppSettings["platform"],
): MarketSearchRow {
  if (platform === "pc") return row;
  return {
    ...row,
    recommendation: {
      ...row.recommendation,
      key: { ...row.recommendation.key, platform },
      fairPrice: null,
      listPrice: null,
      quickSell: null,
      lowestAsk: null,
      depthThree: null,
      depthPrice: null,
      closedVolume: null,
      liveSellOrderCount: 0,
      liveBuyOrderCount: 0,
      confidence: "unknown",
      reasons: [
        {
          code: "no_exact_variant",
          message: "Для выбранной платформы нет bulk-записей; PC-цены не подставляются.",
        },
        {
          code: "no_live_top_buy",
          message: "Активного live buy order ещё нет; выполните явный live-запрос.",
        },
        {
          code: "insufficient_signal",
          message: "Надёжного ценового сигнала нет; PlatScope не подставляет 0p или PC-цену.",
        },
      ],
    },
  };
}

function localizeInventoryItem(item: InventoryViewItem): InventoryViewItem {
  const slug = item.key?.slug ?? item.canonicalGameId;
  return {
    ...item,
    displayName: localizedName(slug, item.displayName),
    key: item.key ? { ...item.key, platform: appSettings.platform } : null,
    closedMedian48h: appSettings.platform === "pc" ? item.closedMedian48h : null,
    hasReliablePrice: appSettings.platform === "pc" && item.hasReliablePrice,
  };
}

function localizeInventoryView(view: InventoryView): InventoryView {
  return { ...view, items: view.items.map(localizeInventoryItem) };
}

export async function installMarketBrowserMock(): Promise<void> {
  mockIPC((command, args) => {
    if (command === "plugin:event|listen") return nextEventListener++;
    if (command === "plugin:event|unlisten") return null;
    if (command === "load_settings") return appSettings;
    if (command === "save_settings") {
      const next = (args as { settings?: AppSettings })?.settings;
      if (!next) throw new Error("settings are required");
      appSettings = { ...next };
      return null;
    }
    if (command === "companion_inventory_status" || command === "check_companion_inventory") {
      return {
        state: appSettings.inventory_companion_enabled ? "missing" : "disabled",
        lastImportedAt: null,
        lastError: null,
      };
    }
    if (command === "foundation_status") {
      return {
        appName: "PlatScope",
        appVersion: "0.1.0",
        databasePath: "C:\\Users\\Demo\\AppData\\Local\\PlatScope\\platscope.db",
        schemaVersion: 9,
        offlineReady: true,
        marketSnapshot: snapshot,
        catalogItemCount: 3840,
        historyCoverage: {
          oldestDate: "2026-08-20",
          newestDate: "2026-08-26",
          dayCount: 7,
        },
        inventoryItemCount: inventory.metadata.itemCount,
      } satisfies FoundationStatus;
    }
    if (command === "diagnostics_status") {
      return {
        generatedAt: "2026-08-27T09:15:00Z",
        foundation: {
          appName: "PlatScope",
          appVersion: "0.1.0",
          databasePath: "C:\\Users\\Demo\\AppData\\Local\\PlatScope\\platscope.db",
          schemaVersion: 9,
          offlineReady: true,
          marketSnapshot: snapshot,
          catalogItemCount: 3840,
          historyCoverage: {
            oldestDate: "2026-08-20",
            newestDate: "2026-08-26",
            dayCount: 7,
          },
          inventoryItemCount: inventory.metadata.itemCount,
        },
        providers: [
          {
            provider: "relics_run",
            lastAttempt: "2026-08-27T09:10:00Z",
            lastSuccess: "2026-08-27T09:10:00Z",
            lastErrorCode: null,
            lastErrorMessage: null,
            latencyMs: 418,
            consecutiveFailures: 0,
          },
          {
            provider: "frame_forge_mirror",
            lastAttempt: "2026-08-27T08:50:00Z",
            lastSuccess: "2026-08-26T08:50:00Z",
            lastErrorCode: "HttpStatus",
            lastErrorMessage: "источник временно вернул ошибку HTTP",
            latencyMs: 732,
            consecutiveFailures: 1,
          },
        ],
      } satisfies DiagnosticsStatus;
    }
    if (command === "export_diagnostics_report") {
      return {
        path: "C:\\Users\\Demo\\AppData\\Local\\PlatScope\\diagnostics\\platscope-diagnostics-20260827T091500000Z.json",
        bytes: 1_842,
      };
    }
    if (command === "account_status") return account;
    if (command === "account_connect") {
      const request = args as { email?: string; password?: string };
      if (!request.email || !request.password) throw new Error("WFM rejected empty credentials");
      account = {
        connected: true,
        profile: {
          id: "demo-user-id",
          ingameName: "DemoTenno",
          slug: "demo-tenno",
          platform: "pc",
          crossplay: true,
          verification: true,
        },
        orders: [
          {
            id: "demo-order-1",
            itemId: "demo-nyx_prime_set",
            type: "sell",
            platinum: 92,
            quantity: 1,
            perTrade: null,
            rank: null,
            charges: null,
            subtype: null,
            amberStars: null,
            cyanStars: null,
            visible: true,
            createdAt: "2026-08-26T08:00:00Z",
            updatedAt: "2026-08-27T06:30:00Z",
          },
        ],
        orderItems: {
          "demo-nyx_prime_set": {
            slug: "nyx_prime_set",
            displayName: localizedName("nyx_prime_set", "Никс Прайм: комплект"),
            imageUrl: "https://warframe.market/static/assets/items/images/en/thumbs/nyx_prime_set.fd41c04c9e9bcc7e0e6963914f68f880.128x128.png",
          },
        },
      };
      return account;
    }
    if (command === "account_disconnect") {
      account = { connected: false, profile: null, orders: [] };
      return true;
    }
    if (command === "account_create_listing") {
      const request = args as { input?: CreateListingInput; confirmed?: boolean };
      if (!request.input || !request.confirmed) throw new Error("explicit confirmation required");
      const now = new Date().toISOString();
      const order: AccountOrder = {
        id: `demo-order-${nextAccountOrder++}`,
        itemId: request.input.itemId,
        type: request.input.type,
        platinum: request.input.platinum,
        quantity: request.input.quantity,
        perTrade: request.input.perTrade,
        rank: request.input.rank,
        charges: request.input.charges,
        subtype: request.input.subtype,
        amberStars: request.input.amberStars,
        cyanStars: request.input.cyanStars,
        visible: request.input.visible,
        createdAt: now,
        updatedAt: now,
      };
      account = { ...account, orders: [...account.orders, order] };
      return order;
    }
    if (command === "account_update_listing") {
      const request = args as { id?: string; input?: UpdateListingInput; confirmed?: boolean };
      if (!request.id || !request.input || !request.confirmed) throw new Error("explicit confirmation required");
      let updated: AccountOrder | null = null;
      account = {
        ...account,
        orders: account.orders.map((order) => {
          if (order.id !== request.id) return order;
          updated = {
            ...order,
            platinum: request.input?.platinum ?? order.platinum,
            quantity: request.input?.quantity ?? order.quantity,
            visible: request.input?.visible ?? order.visible,
            updatedAt: new Date().toISOString(),
          };
          return updated;
        }),
      };
      if (!updated) throw new Error("order not found");
      return updated;
    }
    if (command === "account_delete_listing") {
      const request = args as { id?: string; confirmed?: boolean };
      if (!request.id || !request.confirmed) throw new Error("explicit confirmation required");
      const order = account.orders.find((candidate) => candidate.id === request.id);
      if (!order) throw new Error("order not found");
      account = { ...account, orders: account.orders.filter((candidate) => candidate.id !== request.id) };
      return order;
    }
    if (command === "search_market") {
      const query = String((args as Record<string, unknown>)?.query ?? "").toLocaleLowerCase("ru");
      const matchingRows = rows.filter(
        (row) =>
          row.displayName.toLocaleLowerCase("ru").includes(query) ||
          (englishNames[row.recommendation.key.slug]?.toLocaleLowerCase("en").includes(query) ?? false) ||
          row.recommendation.key.slug.includes(query),
      );
      return {
        query,
        rows: matchingRows.map(localizeMarketRow),
        truncated: false,
        snapshot,
      } satisfies MarketSearchResult;
    }
    if (command === "refresh_market_data") {
      return {
        snapshot,
        catalogItemCount: 3840,
        stale: false,
        usedFallback: false,
        catalogFromCache: false,
        failures: [],
      } satisfies MarketRefreshOutcome;
    }
    if (command === "live_price_current_variant") {
      const key = (args as { key?: MarketSearchRow["recommendation"]["key"] })?.key;
      const row = rows.find((candidate) => candidate.recommendation.key.slug === key?.slug);
      if (!row) return null;
      const scoped = marketRowForPlatform(row, key?.platform as AppSettings["platform"] ?? appSettings.platform);
      const fair = scoped.recommendation.fairPrice;
      return {
        recommendation: {
          ...scoped.recommendation,
          listPrice: fair === null ? 30 : fair + 2,
          quickSell: fair === null ? 18 : Math.max(1, fair - 10),
          lowestAsk: fair === null ? 30 : fair + 2,
          depthThree: fair === null ? 31 : fair + 2.5,
          depthPrice: fair === null ? 32 : fair + 3,
          liveSellOrderCount: 5,
          liveBuyOrderCount: 4,
          confidence: fair === null ? "low" : scoped.recommendation.confidence,
          reasons: [
            ...scoped.recommendation.reasons.filter(
              (reason) => reason.code !== "no_live_top_buy" && reason.code !== "insufficient_signal",
            ),
            ...(fair === null
              ? []
              : [{ code: "live_market_agreement", message: "Live low5 образует согласованный кластер активных sell orders." }]),
            { code: "live_top_buy", message: "Quick Sell основан на лучшем активном buy order точного варианта." },
          ],
        },
        fetchedAt: "2026-08-27T06:45:00Z",
        quoteState: "network",
        sellOrderCount: 5,
        buyOrderCount: 4,
        orders: [
          { side: "sell", platinum: fair === null ? 30 : fair + 2, quantity: 3, perTrade: 1, userStatus: "in_game" },
          { side: "sell", platinum: fair === null ? 32 : fair + 3, quantity: 5, perTrade: 1, userStatus: "online" },
          { side: "buy", platinum: fair === null ? 18 : Math.max(1, fair - 10), quantity: 2, perTrade: 1, userStatus: "in_game" },
        ],
        warning: null,
      } satisfies LivePricingResult;
    }
    if (command === "market_history") {
      const request = args as { key?: MarketSearchRow["recommendation"]["key"]; days?: number };
      if (!request.key) return null;
      if (request.key.platform !== "pc") {
        return {
          key: request.key,
          requestedDays: (request.days ?? 7) as 7 | 30 | 90,
          points: [],
          trend: {
            median7d: null,
            median30d: null,
            median90d: null,
            change7d: null,
            change30d: null,
            volumeAvg7d: null,
            volumeAvg30d: null,
            historicalLow: null,
            historicalHigh: null,
            timing: null,
            trustedDays: 0,
          },
          coverage: {
            oldestDate: "2026-08-20",
            newestDate: "2026-08-26",
            dayCount: 7,
          },
        } satisfies MarketHistoryView;
      }
      const historyPrices = [74, 76, 75, 79, 81, 84, 87];
      return {
        key: request.key,
        requestedDays: (request.days ?? 7) as 7 | 30 | 90,
        points: historyPrices.map((price, index) => ({
          sourceDate: `2026-08-${String(20 + index).padStart(2, "0")}`,
          closedMedian: price,
          closedVolume: 8 + index,
          sellMedian: price + 2,
          buyMedian: price - 8,
        })),
        trend: {
          median7d: 81,
          median30d: null,
          median90d: null,
          change7d: 17.6,
          change30d: null,
          volumeAvg7d: 11,
          volumeAvg30d: null,
          historicalLow: 74,
          historicalHigh: 87,
          timing: "peak",
          trustedDays: 7,
        },
        coverage: {
          oldestDate: "2026-08-20",
          newestDate: "2026-08-26",
          dayCount: 7,
        },
      } satisfies MarketHistoryView;
    }
    if (command === "sell_now") {
      return makeSellNowView();
    }
    if (command === "insights") {
      return makeInsightsView();
    }
    if (command === "refresh_game_metadata") {
      return {
        metadata: makeInsightsView().metadata,
        stale: false,
        usedLkg: false,
        warning: null,
      } satisfies GameMetadataRefreshOutcome;
    }
    if (command === "sell_now_live") {
      const key = (args as { key?: MarketSearchRow["recommendation"]["key"] })?.key;
      const sellNow = makeSellNowView();
      const candidate = sellNow.rows.find((row) => row.inventory.key?.slug === key?.slug);
      if (!candidate?.recommendation) return null;
      const fair = candidate.recommendation.fairPrice;
      const row: SellNowRow = {
        ...candidate,
        recommendation: {
          ...candidate.recommendation,
          listPrice: fair === null ? 30 : fair + 2,
          quickSell: fair === null ? 18 : Math.max(1, fair - 9),
          lowestAsk: fair === null ? 30 : fair + 2,
          depthThree: fair === null ? 31 : fair + 2.5,
          depthPrice: fair === null ? 32 : fair + 3,
          liveSellOrderCount: 5,
          liveBuyOrderCount: 4,
          confidence: fair === null ? "low" : candidate.recommendation.confidence,
          reasons: [
            ...candidate.recommendation.reasons.filter(
              (reason) => reason.code !== "no_live_top_buy" && reason.code !== "insufficient_signal",
            ),
            ...(fair === null
              ? []
              : [{ code: "live_market_agreement", message: "Live low5 подтверждает кластер активных sell orders." }]),
            { code: "live_top_buy", message: "Quick Sell основан на лучшем активном buy order точного варианта." },
          ],
        },
        trend: fair === null ? null : candidate.trend ? { ...candidate.trend, timing: "peak" } : null,
        priority: {
          ...candidate.priority,
          score: Math.min(100, candidate.priority.score + 4),
          reasons: [
            ...candidate.priority.reasons.slice(0, -1),
            "Live-рынок подтвердил момент PEAK; priority пересчитан для точного варианта.",
            "Priority — относительный порядок проверки, а не прогноз платины в день.",
          ],
        },
      };
      return {
        row,
        fetchedAt: "2026-08-27T06:45:00Z",
        quoteState: "network",
        sellOrderCount: 5,
        buyOrderCount: 4,
        warning: null,
      } satisfies LiveSellNowResult;
    }
    if (command === "import_inventory_json") {
      const rawJson = String((args as { rawJson?: string })?.rawJson ?? "");
      if (rawJson.includes('"Inventory"') || rawJson.includes('"ItemType"')) {
        inventory = {
          ...inventory,
          metadata: {
            ...inventory.metadata,
            source: "helper_import",
            observedAt: new Date().toISOString(),
            itemCount: inventory.items.length,
            checksumSha256: "helper-demo",
          },
          summary: {
            ...inventory.summary,
            sellableQuantity: 0,
            attentionRows: inventory.items.length,
          },
          items: inventory.items.map((item) => ({
            ...item,
            tradeableQuantity: 0,
            unknownQuantity: item.ownedQuantity,
            sellableQuantity: 0,
          })),
        };
      }
      return localizeInventoryView(inventory);
    }
    if (command === "load_inventory") {
      return localizeInventoryView(inventory);
    }
    if (command === "set_inventory_keep_copies") {
      const keepCopies = Number((args as { keepCopies?: number })?.keepCopies ?? 1);
      const items = inventory.items.map((item) => ({
        ...item,
        sellableQuantity:
          item.resolution === "resolved" && item.unknownQuantity === 0
            ? Math.min(
                item.tradeableQuantity,
                Math.max(0, item.ownedQuantity - Math.max(keepCopies, item.untradeableQuantity)),
              )
            : 0,
      }));
      inventory = {
        ...inventory,
        keepCopies,
        items,
        summary: {
          ...inventory.summary,
          sellableQuantity: items.reduce((sum, item) => sum + item.sellableQuantity, 0),
        },
      };
      return localizeInventoryView(inventory);
    }
    throw new Error(`Unknown mock command: ${command}`);
  });
}

function makeInsightsView(): InsightsView {
  const platformHasBulk = appSettings.platform === "pc";
  const scopedRecommendation = (slug: string) => {
    const row = rows.find((candidate) => candidate.recommendation.key.slug === slug);
    return row ? marketRowForPlatform(row, appSettings.platform).recommendation : null;
  };
  const componentSpecs = [
    { slug: "nyx_prime_blueprint", requiredQuantity: 1, ownedQuantity: 2, ducats: 25 },
    { slug: "nyx_prime_neuroptics", requiredQuantity: 1, ownedQuantity: 2, ducats: 45 },
    { slug: "nyx_prime_chassis", requiredQuantity: 1, ownedQuantity: 1, ducats: 45 },
    { slug: "nyx_prime_systems", requiredQuantity: 1, ownedQuantity: 1, ducats: 100 },
  ];
  const components: SetComponentInsight[] = componentSpecs.map((component) => ({
    definition: {
      slug: component.slug,
      gameRef: `/Lotus/Demo/${component.slug}`,
      requiredQuantity: component.requiredQuantity,
      ducats: component.ducats,
    },
    ownedQuantity: component.ownedQuantity,
    recommendation: scopedRecommendation(component.slug),
  }));
  const rewards: RelicRewardInsight[] = [
    { name: "Nyx Prime Neuroptics Blueprint", slug: "nyx_prime_neuroptics", chance: 25.33 },
    { name: "Nyx Prime Chassis Blueprint", slug: "nyx_prime_chassis", chance: 25.33 },
    { name: "Nyx Prime Systems Blueprint", slug: "nyx_prime_systems", chance: 11 },
    { name: "Forma Blueprint", slug: null, chance: 38.34 },
  ].map((reward) => ({
    definition: {
      rewardSlug: reward.slug,
      rewardGameRef: `/Lotus/Demo/${reward.name.replaceAll(" ", "")}`,
      displayNameEn: reward.name,
      chancePercent: reward.chance,
    },
    recommendation: reward.slug === null ? null : scopedRecommendation(reward.slug),
  }));
  const setPrice = scopedRecommendation("nyx_prime_set");
  const relicPrice = scopedRecommendation("axi_n10_relic");
  const neuroPrice = scopedRecommendation("nyx_prime_neuroptics");
  return {
    metadata: {
      source: "wfcd_warframe_items",
      fetchedAt: "2026-08-27T07:00:00Z",
      schemaVersion: 3,
      setCount: 162,
      relicCount: 2_184,
      primePartCount: 643,
      rivenDispositionCount: 3,
      itemDefinitionCount: 412,
      checksumSha256: "wfcd-demo",
    },
    inventoryAvailable: true,
    sets: [
      {
        definition: {
          setSlug: "nyx_prime_set",
          setGameRef: "/Lotus/Powersuits/Jade/JadePrime",
          displayNameEn: "Nyx Prime Set",
          vaultStatus: "vaulted",
          components: components.map((component) => component.definition),
        },
        setRecommendation: setPrice,
        comparison: {
          setSlug: "nyx_prime_set",
          completeSets: 1,
          setFairValue: platformHasBulk ? 87 : null,
          partsFairValue: platformHasBulk ? 79 : null,
          setLiquidityAdjustedValue: platformHasBulk ? 36.25 : null,
          partsLiquidityAdjustedValue: platformHasBulk ? 31.4 : null,
          setPremiumPercent: platformHasBulk ? 10.1 : null,
          recommendedMode: platformHasBulk ? "set" : "insufficient_pricing",
          reasons: platformHasBulk
            ? [
                "Из текущих деталей можно собрать комплектов: 1.",
                "Fair set: 87.0p; сумма fair деталей: 79.0p.",
                "После confidence/liquidity комплект сохраняет преимущество более 5%.",
              ]
            : [
                "Для выбранной платформы нет bulk-оценок комплекта и деталей.",
                "PC-цены не используются как запасной источник.",
              ],
        },
        components,
      },
    ],
    relics: [
      {
        definition: {
          relicSlug: "axi_n10_relic",
          relicGameRef: "/Lotus/Types/Game/Projections/AxiN10Bronze",
          displayNameEn: "Axi N10 Relic",
          refinement: "radiant",
          vaultStatus: "vaulted",
          rewards: rewards.map((reward) => reward.definition),
        },
        ownedQuantity: 3,
        sellableQuantity: 2,
        relicRecommendation: relicPrice,
        expectedValue: {
          pricedExpectedValue: platformHasBulk ? 8.1 : null,
          pricedChancePercent: platformHasBulk ? 50.66 : 0,
          totalChancePercent: 100,
          missingRewardCount: platformHasBulk ? 2 : 4,
          coverage: platformHasBulk ? "partial" : "insufficient",
          reasons: platformHasBulk
            ? [
                "Ценами покрыто 50,7% вероятности из 100% описанных наград.",
                "Неоценённых наград: 2; они не заменены фиктивной ценой 1p.",
                "Partial EV показывает только подтверждённую часть и не нормализуется до 100%.",
              ]
            : [
                "Для выбранной платформы нет bulk-оценок наград реликвии.",
                "PC-цены не используются как запасной источник.",
              ],
        },
        rewards,
      },
    ],
    ducats: [
      {
        metadata: {
          slug: "nyx_prime_neuroptics",
          gameRef: "/Lotus/Demo/NyxPrimeNeuroptics",
          ducats: 45,
          vaultStatus: "vaulted",
        },
        displayName: localizedName("nyx_prime_neuroptics", "Никс Прайм: нейрооптика"),
        ownedQuantity: 2,
        sellableQuantity: 1,
        recommendation: neuroPrice,
        efficiency: {
          fairPrice: platformHasBulk ? 24 : null,
          ducats: 45,
          platinumPerDucat: platformHasBulk ? 24 / 45 : null,
          credible: platformHasBulk,
          reasons: platformHasBulk
            ? ["Эффективность рассчитана по credible fair price, а не по единичному low ask."]
            : ["Для выбранной платформы нет credible bulk-цены; PC-цена не подставляется."],
        },
      },
    ],
    rivenDispositions: [
      {
        weaponNameEn: "Soma",
        weaponGameRef: "/Lotus/Weapons/Tenno/Rifle/StartingRifle",
        category: "primary",
        disposition: 4,
        multiplier: 1.2,
      },
      {
        weaponNameEn: "Acceltra Prime",
        weaponGameRef: "/Lotus/Weapons/Tenno/LongGuns/PrimeAcceltra/PrimeAcceltraWeapon",
        category: "primary",
        disposition: 1,
        multiplier: 0.55,
      },
      {
        weaponNameEn: "Kronen Prime",
        weaponGameRef: "/Lotus/Weapons/Tenno/Melee/Tonfa/PrimeTonfa",
        category: "melee",
        disposition: 2,
        multiplier: 0.65,
      },
    ],
  };
}

function makeSellNowView(): SellNowView {
  const scopedInventory = localizeInventoryView(inventory);
  const sellRows = scopedInventory.items
    .filter((item) => item.resolution === "resolved" && item.sellableQuantity > 0 && item.key)
    .map((item): SellNowRow => {
      const marketRow = rows.find((candidate) => candidate.recommendation.key.slug === item.key?.slug);
      const localizedItem = item;
      const recommendation = marketRow
        ? marketRowForPlatform(marketRow, appSettings.platform).recommendation
        : null;
      const isNyx = item.key?.slug === "nyx_prime_set";
      const fair = recommendation?.fairPrice ?? null;
      const priorityScore = isNyx ? 67 : 61;
      return {
        inventory: localizedItem,
        itemKind: marketRow?.itemKind ?? "standard",
        recommendation,
        trend: {
          median7d: isNyx ? 81 : 70,
          median30d: isNyx ? 76 : 68,
          median90d: null,
          change7d: isNyx ? 17.6 : 6.2,
          change30d: isNyx ? 11.5 : 3.4,
          volumeAvg7d: isNyx ? 11 : 45,
          volumeAvg30d: isNyx ? 9 : 41,
          historicalLow: isNyx ? 70 : 62,
          historicalHigh: isNyx ? 87 : 75,
          timing: isNyx ? "peak" : "sell",
          trustedDays: 7,
        },
        priority: {
          score: priorityScore,
          band: "high",
          factors: {
            quantity: item.sellableQuantity / 5,
            price: fair === null ? 0 : fair / (fair + 50),
            liquidity: (recommendation?.closedVolume ?? 0) / ((recommendation?.closedVolume ?? 0) + 10),
            confidenceMultiplier: recommendation?.confidence === "high" ? 1 : 0.75,
            timingMultiplier: isNyx ? 1.05 : 1,
          },
          reasons: [
            `Для продажи подтверждено: ${item.sellableQuantity}; влияние количества ограничено после 5 копий.`,
            `Fair price и закрытые сделки формируют price- и liquidity-факторы.`,
            `Confidence и timing применены как множители; итоговый ranking score ${priorityScore}/100.`,
            "Priority — относительный порядок проверки, а не прогноз платины в день.",
          ],
        },
        nominalValue: fair === null ? null : item.sellableQuantity * fair,
      };
    })
    .sort((left, right) => right.priority.score - left.priority.score);
  return {
    inventoryMetadata: inventory.metadata,
    marketSnapshot: snapshot,
    summary: {
      candidateRows: sellRows.length,
      pricedRows: sellRows.filter((row) => row.recommendation?.fairPrice !== null).length,
      highPriorityRows: sellRows.filter((row) => row.priority.band === "high").length,
      inventoryNominalValue: scopedInventory.items.reduce((sum, item) => {
        const marketRow = rows.find((row) => row.recommendation.key.slug === item.key?.slug);
        const recommendation = marketRow
          ? marketRowForPlatform(marketRow, appSettings.platform).recommendation
          : null;
        return sum + item.ownedQuantity * (recommendation?.fairPrice ?? 0);
      }, 0),
      nominalValue: sellRows.reduce((sum, row) => sum + (row.nominalValue ?? 0), 0),
    },
    rows: sellRows,
  };
}

function makeRow(
  displayName: string,
  slug: string,
  fairPrice: number | null,
  closedVolume: number,
  confidence: PriceConfidence,
  itemKind: MarketItemKind = "standard",
  subtype: string | null = null,
  rank: number | null = null,
): MarketSearchRow {
  return {
    itemId: `demo-${slug}`,
    displayName,
    itemKind,
    masteryRequirement: masteryRequirements[slug] ?? null,
    recommendation: {
      key: {
        slug,
        platform: "pc",
        rank,
        subtype,
        amberStars: null,
        cyanStars: null,
      },
      provider: "relics_run",
      sourceDate,
      fairPrice,
      listPrice: fairPrice,
      quickSell: null,
      lowestAsk: null,
      depthThree: null,
      depthPrice: null,
      closedVolume,
      liveSellOrderCount: 0,
      liveBuyOrderCount: 0,
      confidence,
      freshness: "fresh",
      reasons:
        fairPrice === null
          ? [
              {
                code: "closed_volume_too_low",
                message: "Закрытых сделок недостаточно: 1, требуется не менее 3.",
              },
              {
                code: "insufficient_signal",
                message: "Надёжного ценового сигнала нет; PlatScope не подставляет 0p или 1p.",
              },
            ]
          : [
              {
                code: "source_fresh",
                message: `Bulk snapshot свежий: ${sourceDate}.`,
              },
              {
                code: "trusted_closed_trades",
                message: `Fair baseline подтверждён ${closedVolume} закрытыми сделками: ${fairPrice.toFixed(2)}p.`,
              },
              {
                code: "no_live_top_buy",
                message: "Активного live buy order нет; исторический buy signal не выдан за Quick Sell.",
              },
            ],
    },
  };
}
