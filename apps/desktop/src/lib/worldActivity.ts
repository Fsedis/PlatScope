/** Решения экрана не зависят от формата строк timeLeft и часового пояса Windows. */
export interface ActivityPeriod { activation: string; expiry: string }
export interface ActivityCycle extends ActivityPeriod { key: CycleKey; state: string }
export interface ActivityTrader extends ActivityPeriod { location: string; inventoryIncomplete: boolean }
export interface ActivityOffer {
  gameRef: string; displayName: string; displayNameEn: string;
  kind: "equipment" | "relic" | "other"; ducats: number | null; credits: number | null;
  masteryRef: string | null; setSlug: string | null; relicSlug: string | null;
}
export interface WorldActivityView {
  fetchedAt: string; sourceAt: string; refreshFailed: boolean; catalogAvailable: boolean;
  cycles: ActivityCycle[]; baro: ActivityTrader | null; resurgence: ActivityTrader | null;
  baroOffers: ActivityOffer[]; resurgenceOffers: ActivityOffer[];
  steelPath: (ActivityPeriod & { reward: string; cost: number }) | null;
  sortie: ActivityPeriod | null; events: (ActivityPeriod & { id: string; name: string })[];
  unavailableSections: string[];
}
export const CYCLES = {
  cetus: { name: "Равнины Эйдолона", states: ["day", "night"], region: "cetus" },
  vallis: { name: "Долина Сфер", states: ["cold", "warm"], region: "fortuna" },
  cambion: { name: "Камбионский Дрейф", states: ["fass", "vome"], region: "necralisk" },
  zariman: { name: "Зариман", states: ["grineer", "corpus"], region: null },
  duviri: { name: "Дувири", states: ["sorrow", "fear", "joy", "anger", "envy"], region: null },
} as const;
export type CycleKey = keyof typeof CYCLES;
export type AlertKey = CycleKey | "baro" | "resurgence" | "daily" | "weekly" | "sortie";
export const ALERT_NAMES: Record<AlertKey, string> = {
  cetus: CYCLES.cetus.name, vallis: CYCLES.vallis.name, cambion: CYCLES.cambion.name,
  zariman: CYCLES.zariman.name, duviri: CYCLES.duviri.name, baro: "Баро Ки’Тиир",
  resurgence: "Возрождение Прайм", daily: "Ежедневный сброс", weekly: "Еженедельный сброс", sortie: "Новая вылазка",
};
const STATE_NAMES: Record<string, string> = {
  day: "День", night: "Ночь", cold: "Холод", warm: "Тепло", fass: "Фэз", vome: "Воум",
  corpus: "Корпус", grineer: "Гринир", sorrow: "Печаль", fear: "Страх", joy: "Радость",
  anger: "Гнев", envy: "Зависть", any: "Любая смена", arrival: "Прибытие", departure: "Отбытие",
};
export const stateName = (state: string) => STATE_NAMES[state] ?? "Состояние неизвестно";
export function nextState(cycle: ActivityCycle): string | null {
  const states: readonly string[] = CYCLES[cycle.key]?.states ?? [];
  const index = states.indexOf(cycle.state);
  return index < 0 ? null : states[(index + 1) % states.length];
}
export function periodState(period: ActivityPeriod | null | undefined, now: number): "upcoming" | "active" | "expired" | "missing" {
  if (!period) return "missing";
  const start = Date.parse(period.activation), end = Date.parse(period.expiry);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return "missing";
  return now < start ? "upcoming" : now < end ? "active" : "expired";
}
export function countdown(target: string | number | null | undefined, now: number): string {
  const time = typeof target === "string" ? Date.parse(target) : target;
  if (time == null || !Number.isFinite(time)) return "—";
  if (time <= now) return "Уточняем…";
  const total = Math.ceil((time - now) / 1000), days = Math.floor(total / 86400);
  const hours = Math.floor(total / 3600) % 24, minutes = Math.floor(total / 60) % 60, seconds = total % 60;
  if (days) return `${days} д ${hours} ч`;
  if (hours) return `${hours} ч ${minutes.toString().padStart(2, "0")} мин`;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
export function nextReset(now: number, weekly = false): number {
  const date = new Date(now);
  const today = Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate());
  if (!weekly) return today + 86_400_000;
  const days = (8 - date.getUTCDay()) % 7 || 7;
  return today + days * 86_400_000;
}
export function sectionStale(view: WorldActivityView, key: string, now: number): boolean {
  return view.refreshFailed || view.unavailableSections.includes(key)
    || !Number.isFinite(Date.parse(view.sourceAt)) || now - Date.parse(view.sourceAt) > 20 * 60_000
    || Date.parse(view.sourceAt) - now > 5 * 60_000;
}
export function nextWorldRefresh(view: WorldActivityView, now: number): number {
  if (view.refreshFailed || view.unavailableSections.length || sectionStale(view, "", now)) return now + 45_000;
  if ((periodState(view.baro, now) === "active" && (!view.baroOffers.length || view.baro?.inventoryIncomplete))
    || (periodState(view.resurgence, now) === "active" && (!view.resurgenceOffers.length || view.resurgence?.inventoryIncomplete))) return now + 45_000;
  const periods = [...view.cycles, view.baro, view.resurgence, view.steelPath, view.sortie];
  const boundaries = periods.flatMap(period => {
    if (!period) return [];
    return [Date.parse(period.activation), Date.parse(period.expiry)].filter(time => Number.isFinite(time) && time > now);
  });
  if (periods.some(period => period && Date.parse(period.expiry) <= now)) return now + 45_000;
  return Math.max(now + 15_000, Math.min(now + 15 * 60_000, ...boundaries) + 3_000);
}
export function traderLocation(location: string): string {
  return location.replace("Strata Relay (Earth)", "Реле Страта · Земля")
    .replace("Larunda Relay (Mercury)", "Реле Ларунда · Меркурий")
    .replace("Kronia Relay (Saturn)", "Реле Крония · Сатурн")
    .replace("Orcus Relay (Pluto)", "Реле Оркус · Плутон")
    .replace("Maroo's Bazaar (Mars)", "Базар Мэру · Марс");
}
export function steelReward(name: string): string {
  return ({ "Rifle Riven Mod": "Мод Разлома для винтовки", "Shotgun Riven Mod": "Мод Разлома для дробовика",
    "Pistol Riven Mod": "Мод Разлома для пистолета", "Melee Riven Mod": "Мод Разлома для ближнего боя",
    "Umbra Forma Blueprint": "Чертёж Формы Умбра", "3x Forma": "3 Формы", "Forma Bundle": "Набор Форм",
    "50,000 Kuva": "50 000 Кувы", "30,000 Endo": "30 000 Эндо" } as Record<string, string>)[name] ?? name;
}
export function offerCost(offer: ActivityOffer, resurgence: boolean): string {
  const number = (value: number) => value.toLocaleString("ru-RU");
  const costs: string[] = [];
  if (offer.ducats != null && offer.ducats > 0) costs.push(`${number(offer.ducats)} ${resurgence ? "Королевской Ая" : "дукатов"}`);
  if (offer.credits != null && offer.credits > 0) costs.push(`${number(offer.credits)} ${resurgence ? "Ая" : "кредитов"}`);
  return costs.join(" + ") || "Цена не указана";
}

export interface WorldAlertRule {
  id: string; key: AlertKey; state: string; leadMinutes: 0 | 5;
  repeat: boolean; enabled: boolean; createdAt: number;
}
export interface WorldPreferences { startHere: boolean; rules: WorldAlertRule[]; sent: string[] }
export const WORLD_PREFERENCES_KEY = "platscope.world-activity.v1";
export function alertStates(key: AlertKey): string[] {
  if (key in CYCLES) return ["any", ...CYCLES[key as CycleKey].states];
  if (key === "baro") return ["arrival", "departure"];
  return ["any"];
}
export function parseWorldPreferences(raw: string | null): WorldPreferences {
  const empty = { startHere: false, rules: [], sent: [] };
  try {
    const value = JSON.parse(raw ?? "null");
    if (!value || typeof value !== "object") return empty;
    const rules: WorldAlertRule[] = Array.isArray(value.rules) ? value.rules.filter((rule: WorldAlertRule) => rule
      && typeof rule.id === "string" && rule.id.length <= 100 && Object.hasOwn(ALERT_NAMES, rule.key)
      && alertStates(rule.key).includes(rule.state) && [0, 5].includes(rule.leadMinutes)
      && typeof rule.repeat === "boolean" && typeof rule.enabled === "boolean" && Number.isFinite(rule.createdAt)).slice(0, 30) : [];
    return { startHere: value.startHere === true,
      rules: rules.filter((rule, index) => rules.findIndex(other => other.id === rule.id || (other.key === rule.key && other.state === rule.state)) === index),
      sent: Array.isArray(value.sent) ? value.sent.filter((id: unknown) => typeof id === "string" && id.length < 250).slice(-256) : [] };
  } catch { return empty; }
}

export interface AlertCandidate { id: string; ruleId: string; title: string; body: string }
export function markWorldAlertsDelivered(preferences: WorldPreferences, alerts: AlertCandidate[]): WorldPreferences {
  const fired = new Set(alerts.map(alert => alert.ruleId));
  return { ...preferences, sent: [...preferences.sent, ...alerts.map(alert => alert.id)].slice(-256),
    rules: preferences.rules.map(rule => fired.has(rule.id) && !rule.repeat ? { ...rule, enabled: false } : rule) };
}
/** После сна не догоняем старые события пачкой. Повторяемое правило означает один раз за подходящий цикл. */
export function worldAlertCandidates(view: WorldActivityView | null, preferences: WorldPreferences, now: number, previousTick: number, eligibleSince = -Infinity): AlertCandidate[] {
  const result: AlertCandidate[] = [];
  for (const rule of preferences.rules) {
    if (!rule.enabled) continue;
    let eventAt: number | null = null, state = "any";
    if (rule.key === "daily" || rule.key === "weekly") {
      eventAt = nextReset(now, rule.key === "weekly") - (rule.leadMinutes ? 0 : (rule.key === "weekly" ? 7 : 1) * 86_400_000);
    } else {
      if (!view || sectionStale(view, rule.key, now)) continue;
      if (rule.key in CYCLES) {
        const cycle = view.cycles.find(cycle => cycle.key === rule.key);
        if (!cycle || periodState(cycle, now) !== "active") continue;
        state = rule.leadMinutes ? nextState(cycle) ?? "" : cycle.state;
        eventAt = Date.parse(rule.leadMinutes ? cycle.expiry : cycle.activation);
      } else if (rule.key === "baro") {
        if (!view.baro) continue;
        state = rule.state;
        eventAt = Date.parse(rule.state === "arrival" ? view.baro.activation : view.baro.expiry);
      } else {
        const period = rule.key === "resurgence" ? view.resurgence : view.sortie;
        if (!period) continue;
        eventAt = Date.parse(rule.leadMinutes ? period.expiry : period.activation);
      }
    }
    if (!eventAt || !Number.isFinite(eventAt) || (rule.state !== "any" && rule.state !== state)) continue;
    const triggerAt = eventAt - rule.leadMinutes * 60_000;
    // При старте подтверждение новой ротации может прийти с небольшой задержкой источника.
    const tolerance = rule.leadMinutes ? 60_000 : 120_000;
    if (triggerAt < Math.max(rule.createdAt, eligibleSince) || triggerAt > now || now - triggerAt > tolerance
      || previousTick > now || now - previousTick > 2 * 60_000) continue;
    const id = `${rule.id}:${state}:${Math.floor(eventAt / 60_000)}:${rule.leadMinutes}`;
    if (preferences.sent.includes(id)) continue;
    result.push({ id, ruleId: rule.id, title: ALERT_NAMES[rule.key],
      body: rule.leadMinutes ? `Через 5 минут: ${state === "any" ? "смена ротации" : stateName(state).toLocaleLowerCase("ru")}.`
        : rule.key === "daily" ? "Ежедневные лимиты обновились." : rule.key === "weekly" ? "Началась новая игровая неделя."
        : state === "any" ? "Ротация обновилась." : rule.key === "baro"
          ? (state === "arrival" ? "Баро прибыл. Можно посмотреть его товары." : "Баро покинул реле.")
          : `Сейчас: ${stateName(state).toLocaleLowerCase("ru")}.` });
  }
  return result;
}
