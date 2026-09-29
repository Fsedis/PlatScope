import type { CraftingIngredient, CraftingRecipe, MasteryPlanItem, MasteryPlanView } from "./masteryPlan";

/** Репрезентативный экран браузера. Рецепты и цены основаны на публичном экспорте, запасы демонстрационные. */
export function makeMasteryPlanMock(refs: string[] | null, scenario: string | null): MasteryPlanView {
  const image = (name: string) => `https://cdn.warframestat.us/img/${name}.png`;
  const material = (key: string, ru: string, en: string, quantity: number, art: string): CraftingIngredient => ({
    gameRef:`/Lotus/Types/Items/MiscItems/${key}`,displayNameEn:en,displayNameRu:ru,imageUrl:image(art),
    quantity,equipment:false,drops:[],
  });
  const alloy = (n: number) => material("AlloyPlate","Сплавы","Alloy Plate",n,"AlloyPlate");
  const neurodes = (n: number) => material("Neurode","Нейроды","Neurodes",n,"ComponentNeurode");
  const salvage = (n: number) => material("Salvage","Вторсырьё","Salvage",n,"ComponentSalvage");
  const polymer = (n: number) => material("PolymerBundle","Полимеры","Polymer Bundle",n,"PolymerBundle");
  const seed = (ref: string,ru: string,en: string,category = "primary",owned = 0,rank = 0): MasteryPlanItem => ({
    gameRef:ref,displayName:ru,displayNameEn:en,imageUrl:image(en.replaceAll(" ","")),category,
    ownedQuantity:owned,masteryRank:rank,maxRank:30,remainingMasteryPoints:(30-rank)*(category === "warframe" ? 200 : 100),
    state:owned ? "owned" : "gather",recipe:null,blueprintOwned:0,materials:[],missingTypes:0,totalCredits:null,missingCredits:null,rankBlocked:false,
  });
  const braton = seed("/Lotus/Weapons/Tenno/Rifle/Rifle","Брэйтон","Braton","primary",1,12);
  const rhino = seed("/Lotus/Powersuits/Rhino/Rhino","Рино","Rhino","warframe",1,0);
  const boltor = seed("/Lotus/Weapons/Tenno/Rifle/BoltoRifle","Болтор","Boltor");
  const hek = seed("/Lotus/Weapons/Tenno/Shotgun/QuadShotgun","Хек","Hek");
  const paris = seed("/Lotus/Weapons/Tenno/Bows/HuntingBow","Парис","Paris");
  const ignis = seed("/Lotus/Weapons/ClanTech/Chemical/FlameThrower","Игнис","Ignis");
  const soma = seed("/Lotus/Weapons/Tenno/Rifle/TennoAR","Сома","Soma");
  const coda = seed("/Lotus/Demo/HemaCoda","Хема Кода","Coda Hema","primary",1,30); coda.maxRank = 40;coda.remainingMasteryPoints = 1000;coda.imageUrl = image("Hema");
  const addRecipe = (item: MasteryPlanItem,bp: string,price: number | null,build: number,time: number,mr: number,ingredients: CraftingIngredient[],source: CraftingRecipe["blueprintSource"] = "market",owned = 0) => {
    item.recipe = {resultGameRef:item.gameRef,blueprintGameRef:bp,blueprintConsumed:true,blueprintSource:source,blueprintPrice:price,
      buildPrice:build,buildTimeSeconds:time,masteryRequirement:mr,ingredients,blueprintDrops:[]};item.blueprintOwned = owned;
  };
  addRecipe(boltor,"/Lotus/Types/Recipes/Weapons/BoltorBlueprint",15000,25000,86400,2,[alloy(100),neurodes(2),salvage(900),polymer(600)]);
  addRecipe(hek,"/Lotus/Types/Recipes/Weapons/QuadShotgunBlueprint",25000,25000,86400,4,[material("Circuits","Электросхемы","Circuits",900,"Circuits"),neurodes(5),material("Rubedo","Рубедо","Rubedo",1000,"Rubedo"),salvage(1200)],"market",1);
  addRecipe(paris,"/Lotus/Types/Recipes/Weapons/HuntingBowBlueprint",20000,15000,43200,0,[material("Morphic","Морфиды","Morphics",3,"Morphics"),material("Nanospores","Наноспоры","Nano Spores",2000,"Nanospores"),material("Plastids","Пластиды","Plastids",600,"Plastids"),polymer(700)]);
  addRecipe(ignis,"/Lotus/Weapons/ClanTech/Chemical/FlameThrowerBlueprint",15000,30000,86400,5,[material("Ferrite","Феррит","Ferrite",5000,"Ferrite"),material("Forma","Форма","Forma",1,"Forma"),material("ChemComponent","Детонитовый инжектор","Detonite Injector",2,"DetoniteInjector")],"dojo");
  addRecipe(soma,"/Lotus/Demo/SomaBlueprint",50000,25000,86400,6,[material("Morphic","Морфиды","Morphics",7,"Morphics"),salvage(8500),polymer(1200)]);
  const stocks: Record<string,number> = {AlloyPlate:450,Neurode:5,Salvage:5000,PolymerBundle:950,Circuits:1100,Rubedo:1450,Morphic:1,Nanospores:7000,Plastids:800,Ferrite:6000,Forma:1,ChemComponent:2};
  const original = Object.fromEntries(Object.entries(stocks).map(([key,value]) => [`/Lotus/Types/Items/MiscItems/${key}`,value]));
  const credits = 98000;
  function evaluate(item: MasteryPlanItem,budget: Record<string,number>,balance: number): MasteryPlanItem {
    const copy = structuredClone(item);
    if (!copy.recipe || copy.ownedQuantity) return copy;
    copy.materials = copy.recipe.ingredients.map(definition => ({definition,ownedQuantity:original[definition.gameRef] ?? 0,
      availableQuantity:budget[definition.gameRef] ?? 0,protectedQuantity:0,missingQuantity:Math.max(0,definition.quantity-(budget[definition.gameRef] ?? 0))}));
    copy.missingTypes = copy.materials.filter(material => material.missingQuantity > 0).length;
    copy.totalCredits = copy.recipe.buildPrice + (copy.blueprintOwned ? 0 : copy.recipe.blueprintPrice ?? 0);
    copy.missingCredits = Math.max(0,copy.totalCredits-balance);
    copy.state = copy.missingTypes === 0 && copy.missingCredits === 0 && copy.blueprintOwned ? "craft"
      : copy.missingTypes === 0 && copy.missingCredits === 0 && copy.recipe.blueprintSource === "market" ? "buy_blueprint"
      : copy.missingTypes === 1 && (copy.blueprintOwned || copy.recipe.blueprintSource === "market") ? "one_short" : "gather";
    return copy;
  }
  const seeds = [braton,rhino,coda,hek,boltor,paris,ignis,soma];
  const savedRefs = scenario === "empty" ? [] : refs ?? [braton.gameRef,boltor.gameRef,hek.gameRef];
  const budget = {...original};let balance = credits;
  const queue = savedRefs.flatMap(ref => {
    const seed = seeds.find(item => item.gameRef === ref);if (!seed) return [];
    const row = evaluate(seed,budget,balance);
    if (row.recipe && !row.ownedQuantity) { row.materials.forEach(material => budget[material.definition.gameRef] = Math.max(0,(budget[material.definition.gameRef] ?? 0)-material.definition.quantity));balance = Math.max(0,balance-(row.totalCredits ?? 0)); }
    return [row];
  });
  const view: MasteryPlanView = {inventoryChecksum:"demo-mastery-plan",inventoryAvailable:true,historyAvailable:true,recipesAvailable:true,refreshFailed:false,
    observedAt:"2026-09-30T03:15:00Z",metadataAt:"2026-09-30T01:00:00Z",credits,accountRank:12,savedRefs,
    candidates:seeds.map(item => queue.find(row => row.gameRef === item.gameRef) ?? evaluate(item,original,credits)),queue};
  const priority = {owned:0,craft:1,buy_blueprint:2,one_short:3,gather:4,rank_locked:5,access_unknown:6,no_recipe:7,unknown:8,mastered:8};
  view.candidates.sort((a,b) => priority[a.state]-priority[b.state] || a.missingTypes-b.missingTypes || (a.totalCredits ?? Infinity)-(b.totalCredits ?? Infinity) || a.displayName.localeCompare(b.displayName,"ru"));
  if (scenario === "none") { view.historyAvailable = false;view.inventoryAvailable = false;view.observedAt = null; }
  if (scenario === "catalog") { view.recipesAvailable = false;view.candidates = view.candidates.filter(item => item.ownedQuantity); }
  if (scenario === "stale") view.refreshFailed = true;
  return view;
}
