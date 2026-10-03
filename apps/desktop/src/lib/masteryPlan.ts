export interface CraftingDropSource { location: string; chancePercent: number }
export interface CraftingIngredient {
  gameRef: string; displayNameEn: string; displayNameRu: string | null; imageUrl: string | null;
  quantity: number; equipment: boolean; drops: CraftingDropSource[];
}
export interface CraftingRecipe {
  resultGameRef: string; blueprintGameRef: string; blueprintConsumed: boolean;
  blueprintSource: "market" | "dojo" | "unknown"; blueprintPrice: number | null;
  masteryRequirement: number | null; buildPrice: number; buildTimeSeconds: number; resultQuantity: number;
  ingredients: CraftingIngredient[]; blueprintDrops: CraftingDropSource[];
}
export type MasteryPlanState = "owned" | "craft" | "buy_blueprint" | "crafting" | "ready_to_claim" | "waiting_blueprint" | "waiting_components" | "prepare_components" | "one_short" | "gather" | "rank_locked" | "access_unknown" | "no_recipe" | "unknown" | "mastered";
export interface MasteryPlanFoundry {
  quantity: number; readyQuantity: number; unknownCompletionQuantity: number; completesAt: string | null;
}
export interface MasteryPlanComponent {
  recipe: CraftingRecipe; batches: number; requiredQuantity: number; producedQuantity: number; blueprintOwned: number; blueprintPlanned: boolean;
  materials: MasteryPlanMaterial[]; missingTypes: number; knownCredits: number; totalCredits: number | null;
  missingCredits: number | null; state: MasteryPlanState; rankBlocked: boolean;
}
export interface MasteryPlanMaterial {
  definition: CraftingIngredient; ownedQuantity: number; availableQuantity: number;
  protectedQuantity: number; missingQuantity: number; pendingQuantity: number; plannedQuantity: number;
  foundry: MasteryPlanFoundry | null; component: MasteryPlanComponent | null;
  componentIssue: "ambiguous_recipe" | "invalid_recipe" | null;
}
export interface MasteryPlanItem {
  gameRef: string; displayName: string; displayNameEn: string; imageUrl: string | null; category: string;
  ownedQuantity: number; masteryRank: number | null; maxRank: number | null; remainingMasteryPoints: number | null;
  state: MasteryPlanState; recipe: CraftingRecipe | null; blueprintOwned: number; blueprintPlanned: boolean; materials: MasteryPlanMaterial[];
  missingTypes: number; knownCredits: number; totalCredits: number | null; missingCredits: number | null; rankBlocked: boolean;
  foundry: MasteryPlanFoundry | null;
}
export interface MasteryPlanView {
  inventoryChecksum: string | null;
  inventoryAvailable: boolean; historyAvailable: boolean; recipesAvailable: boolean; refreshFailed: boolean;
  foundryAvailable: boolean; foundryIssue: boolean;
  observedAt: string | null; metadataAt: string | null; credits: number | null; accountRank: number | null;
  savedRefs: string[]; candidates: MasteryPlanItem[]; queue: MasteryPlanItem[];
  knownPlanCredits: number; totalPlanCredits: number | null; missingPlanCredits: number | null; planMaterials: MasteryPlanMaterial[];
}
export type BlueprintSourceFilter = "all" | "market" | "dojo" | "drops" | "unknown";
export function matchesBlueprintSource(item: MasteryPlanItem, source: BlueprintSourceFilter): boolean {
  if (source === "all") return true;
  if (source === "drops") return Boolean(item.recipe?.blueprintDrops.length);
  if (source === "unknown") return !item.recipe
    || (item.recipe.blueprintSource === "unknown" && item.recipe.blueprintDrops.length === 0);
  return item.recipe?.blueprintSource === source;
}
export const planLabels: Record<MasteryPlanState,string> = {
  owned: "Уже есть · осталось освоить", craft: "Можно изготовить", buy_blueprint: "Купить чертёж · материалы есть",
  crafting: "Изготавливается", ready_to_claim: "Готово — заберите", waiting_components: "Ожидает компоненты из кузницы",
  waiting_blueprint: "Чертёж учтён ранее в плане",
  prepare_components: "Нужно изготовить компоненты",
  one_short: "Не хватает одного материала", gather: "Нужно подготовить материалы", rank_locked: "Нужен ранг мастерства",
  access_unknown: "Проверьте условия доступа",
  no_recipe: "Нет подтверждённого рецепта", unknown: "Нужны данные аккаунта", mastered: "Уже освоено",
};
export function planStatusLabel(item: MasteryPlanItem): string {
  if (item.state === "gather" && item.missingTypes === 0) {
    if (item.blueprintOwned === 0 && item.recipe?.blueprintSource === "dojo") return "Чертёж в додзё";
    if (item.blueprintOwned === 0 && item.recipe?.blueprintSource === "unknown") return "Нужен чертёж";
    if (item.missingCredits) return "Не хватает кредитов";
    if (item.missingCredits === null) return "Проверьте запас кредитов";
  }
  return planLabels[item.state];
}
export function planNextStep(item: MasteryPlanItem): string {
  if (item.state === "mastered") return "Предмет освоен. Можно убрать его из плана.";
  if (item.state === "unknown") return "Обновите инвентарь и историю освоения из Warframe.";
  if (item.state === "ready_to_claim") return "Заберите предмет из кузницы в игре, затем обновите инвентарь и прокачайте его на миссиях.";
  if (item.state === "crafting") return "Дождитесь окончания изготовления и заберите предмет из кузницы. После получения обновите инвентарь.";
  if (item.ownedQuantity > 0) return item.maxRank === 40
    ? "Прокачайте имеющийся предмет. Для рангов выше 30 могут потребоваться дополнительные Формы."
    : "Возьмите имеющийся предмет на миссию и прокачайте до максимального ранга.";
  if (!item.recipe) return "Способ получения не подтверждён в сохранённых данных.";
  if (item.rankBlocked) return `Сначала достигните ранга мастерства ${item.recipe.masteryRequirement}.`;
  if (item.state === "access_unknown") return "Проверьте требуемый ранг мастерства и доступность предмета в игре.";
  if (item.state === "waiting_blueprint") return "Получите чертёж по предыдущему шагу плана. Его стоимость уже включена в план; после получения обновите инвентарь.";
  if (item.state === "waiting_components") return "Заберите готовые компоненты или дождитесь их изготовления в кузнице. Затем обновите инвентарь.";
  if (item.state === "prepare_components") return "Откройте недостающие компоненты в материалах: подготовьте их сырьё и изготовьте в кузнице.";
  if (item.blueprintOwned === 0) {
    if (item.blueprintPlanned) return "Получите чертёж по предыдущему шагу плана. Его стоимость уже включена в план; после получения обновите инвентарь.";
    if (item.recipe.blueprintSource === "market") return "Купите чертёж в игровом магазине за кредиты.";
    if (item.recipe.blueprintSource === "dojo") return "Проверьте исследование в додзё и скопируйте чертёж.";
    return "Получите чертёж. Его покупка в игровом магазине не подтверждена.";
  }
  if (item.missingTypes) return "Соберите недостающие материалы, затем изготовьте предмет в кузнице.";
  if (item.missingCredits) return "Накопите недостающие кредиты для изготовления.";
  if (item.missingCredits === null) return "Проверьте запас кредитов и условия доступа в игре.";
  return "Изготовьте предмет в кузнице, затем прокачайте его на миссиях.";
}
export function planDuration(seconds: number): string {
  if (seconds >= 86400) return `${Number((seconds / 86400).toFixed(1))} дн.`;
  if (seconds >= 3600) return `${Number((seconds / 3600).toFixed(1))} ч`;
  return `${Math.ceil(seconds / 60)} мин`;
}
