import type { CraftingIngredient, CraftingRecipe, MasteryPlanFoundry, MasteryPlanItem, MasteryPlanMaterial, MasteryPlanState, MasteryPlanView } from "./masteryPlan";

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
    state:owned ? "owned" : "gather",recipe:null,blueprintOwned:0,blueprintPlanned:false,materials:[],missingTypes:0,knownCredits:0,totalCredits:owned ? 0 : null,missingCredits:owned ? 0 : null,rankBlocked:false,foundry:null,
  });
  const braton = seed("/Lotus/Weapons/Tenno/Rifle/Rifle","Брэйтон","Braton","primary",1,12);
  const rhino = seed("/Lotus/Powersuits/Rhino/Rhino","Рино","Rhino","warframe",1,0);
  const boltor = seed("/Lotus/Weapons/Tenno/Rifle/BoltoRifle","Болтор","Boltor");
  const hek = seed("/Lotus/Weapons/Tenno/Shotgun/QuadShotgun","Хек","Hek");
  const paris = seed("/Lotus/Weapons/Tenno/Bows/HuntingBow","Парис","Paris");
  const ignis = seed("/Lotus/Weapons/ClanTech/Chemical/FlameThrower","Игнис","Ignis");
  const soma = seed("/Lotus/Weapons/Tenno/Rifle/TennoAR","Сома","Soma");
  const jat = seed("/Lotus/Weapons/ClanTech/Chemical/RocketHammer","Джет Киттаг","Jat Kittag","melee");
  const coda = seed("/Lotus/Demo/HemaCoda","Хема Кода","Coda Hema","primary",1,30); coda.maxRank = 40;coda.remainingMasteryPoints = 1000;coda.imageUrl = image("Hema");
  const addRecipe = (item: MasteryPlanItem,bp: string,price: number | null,build: number,time: number,mr: number,ingredients: CraftingIngredient[],source: CraftingRecipe["blueprintSource"] = "market",owned = 0) => {
    item.recipe = {resultGameRef:item.gameRef,blueprintGameRef:bp,blueprintConsumed:true,blueprintSource:source,blueprintPrice:price,
      buildPrice:build,buildTimeSeconds:time,resultQuantity:1,masteryRequirement:mr,ingredients,blueprintDrops:[]};item.blueprintOwned = owned;
  };
  addRecipe(boltor,"/Lotus/Types/Recipes/Weapons/BoltorBlueprint",15000,25000,86400,2,[alloy(100),neurodes(2),salvage(900),polymer(600)]);
  addRecipe(hek,"/Lotus/Types/Recipes/Weapons/QuadShotgunBlueprint",25000,25000,86400,4,[material("Circuits","Электросхемы","Circuits",900,"Circuits"),neurodes(5),material("Rubedo","Рубедо","Rubedo",1000,"Rubedo"),salvage(1200)],"market",1);
  addRecipe(paris,"/Lotus/Types/Recipes/Weapons/HuntingBowBlueprint",20000,15000,43200,0,[material("Morphic","Морфиды","Morphics",3,"Morphics"),material("Nanospores","Наноспоры","Nano Spores",2000,"Nanospores"),material("Plastids","Пластиды","Plastids",600,"Plastids"),polymer(700)]);
  addRecipe(ignis,"/Lotus/Weapons/ClanTech/Chemical/FlameThrowerBlueprint",15000,30000,86400,5,[material("Ferrite","Феррит","Ferrite",5000,"Ferrite"),material("Forma","Форма","Forma",1,"Forma"),material("ChemComponent","Детонитовый инжектор","Detonite Injector",2,"DetoniteInjector")],"dojo");
  addRecipe(soma,"/Lotus/Demo/SomaBlueprint",50000,25000,86400,6,[material("Morphic","Морфиды","Morphics",7,"Morphics"),salvage(8500),polymer(1200)]);
  addRecipe(jat,"/Lotus/Demo/JatKittagBlueprint",30000,30000,86400,7,[material("Ferrite","Феррит","Ferrite",5000,"Ferrite"),material("Forma","Форма","Forma",1,"Forma"),material("ChemComponent","Детонитовый инжектор","Detonite Injector",5,"DetoniteInjector"),material("Rubedo","Рубедо","Rubedo",600,"Rubedo")],"dojo",1);
  const injector = material("ChemComponent","Детонитовый инжектор","Detonite Injector",1,"DetoniteInjector");
  const componentRecipes: Record<string,CraftingRecipe> = {[injector.gameRef]:{
    resultGameRef:injector.gameRef,blueprintGameRef:"/Lotus/Demo/DetoniteInjectorBlueprint",blueprintConsumed:false,blueprintSource:"dojo",blueprintPrice:15000,
    masteryRequirement:0,buildPrice:15000,buildTimeSeconds:43200,resultQuantity:1,
    ingredients:[material("DetoniteAmpule","Детонитовые ампулы","Detonite Ampule",10,"DetoniteAmpule"),material("Ferrite","Феррит","Ferrite",500,"Ferrite"),polymer(250),material("Plastids","Пластиды","Plastids",250,"Plastids")],blueprintDrops:[],
  }};
  const stocks: Record<string,number> = {AlloyPlate:450,Neurode:5,Salvage:5000,PolymerBundle:950,Circuits:1100,Rubedo:1450,Morphic:1,Nanospores:7000,Plastids:800,Ferrite:6000,Forma:1,ChemComponent:0,DetoniteAmpule:35};
  const original = Object.fromEntries(Object.entries(stocks).map(([key,value]) => [`/Lotus/Types/Items/MiscItems/${key}`,value]));
  const credits = 98000;
  const future = new Date(Date.now()+2*3600_000).toISOString();
  const foundries: Record<string,MasteryPlanFoundry> = scenario === "no_foundry" ? {} : {
    [boltor.gameRef]:{quantity:1,readyQuantity:0,unknownCompletionQuantity:0,completesAt:future},
    [hek.gameRef]:{quantity:1,readyQuantity:1,unknownCompletionQuantity:0,completesAt:null},
    [injector.gameRef]:{quantity:1,readyQuantity:0,unknownCompletionQuantity:0,completesAt:future},
    ["/Lotus/Types/Items/MiscItems/Forma"]:{quantity:1,readyQuantity:1,unknownCompletionQuantity:0,completesAt:null},
  };
  type Budget = {stocks:Record<string,number>;pending:Record<string,number>;planned:Record<string,number>};
  const freshBudget = (): Budget => ({stocks:{...original},pending:Object.fromEntries(Object.entries(foundries).map(([ref,foundry]) => [ref,foundry.quantity])),planned:{}});
  function evaluateMaterial(definition: CraftingIngredient,budget: Budget): MasteryPlanMaterial {
    const ref = definition.gameRef;
    const availableQuantity = budget.stocks[ref] ?? 0;
    const used = Math.min(availableQuantity,definition.quantity);
    const pendingQuantity = Math.min(budget.pending[ref] ?? 0,definition.quantity-used);
    const plannedQuantity = Math.min(budget.planned[ref] ?? 0,definition.quantity-used-pendingQuantity);
    budget.stocks[ref] = Math.max(0,availableQuantity-used);
    budget.pending[ref] = Math.max(0,(budget.pending[ref] ?? 0)-pendingQuantity);
    budget.planned[ref] = Math.max(0,(budget.planned[ref] ?? 0)-plannedQuantity);
    return {definition,ownedQuantity:original[ref] ?? 0,availableQuantity,protectedQuantity:0,missingQuantity:definition.quantity-used-pendingQuantity-plannedQuantity,
      pendingQuantity,plannedQuantity,foundry:pendingQuantity ? foundries[ref] : null,component:null,componentIssue:null};
  }
  function evaluate(item: MasteryPlanItem,budget: Budget,balance: number): MasteryPlanItem {
    const copy = structuredClone(item);
    if (!copy.recipe || copy.ownedQuantity) return copy;
    copy.foundry = foundries[copy.gameRef] ?? null;
    if (copy.foundry) {
      copy.state = copy.foundry.readyQuantity ? "ready_to_claim" : "crafting";copy.materials = [];copy.totalCredits = 0;copy.knownCredits = 0;copy.missingCredits = 0;return copy;
    }
    copy.materials = copy.recipe.ingredients.map(definition => evaluateMaterial(definition,budget));
    copy.knownCredits = copy.recipe.buildPrice+(copy.blueprintOwned ? 0 : copy.recipe.blueprintPrice ?? 0);
    for (const ingredient of copy.materials) {
      const recipe = componentRecipes[ingredient.definition.gameRef];
      if (!recipe || !ingredient.missingQuantity) continue;
      const batches = Math.ceil(ingredient.missingQuantity/recipe.resultQuantity);
      const materials = recipe.ingredients.map(definition => evaluateMaterial({...definition,quantity:definition.quantity*batches},budget));
      const missingTypes = materials.filter(material => material.missingQuantity > 0).length;
      const knownCredits = recipe.buildPrice*batches;
      ingredient.component = {recipe,materials,batches,requiredQuantity:ingredient.missingQuantity,producedQuantity:recipe.resultQuantity*batches,blueprintOwned:1,blueprintPlanned:false,
        missingTypes,knownCredits,totalCredits:knownCredits,missingCredits:Math.max(0,knownCredits-balance),state:missingTypes ? "gather" : "craft",rankBlocked:false};
      copy.knownCredits += knownCredits;
      budget.planned[ingredient.definition.gameRef] = (budget.planned[ingredient.definition.gameRef] ?? 0)+Math.max(0,recipe.resultQuantity*batches-ingredient.missingQuantity);
    }
    copy.missingTypes = copy.materials.filter(material => material.missingQuantity > 0).length;
    copy.totalCredits = copy.knownCredits;
    copy.missingCredits = Math.max(0,copy.totalCredits-balance);
    copy.state = copy.missingTypes === 0 && copy.missingCredits === 0 && copy.blueprintOwned ? "craft"
      : copy.missingTypes === 0 && copy.missingCredits === 0 && copy.recipe.blueprintSource === "market" ? "buy_blueprint"
      : copy.missingTypes === 1 && (copy.blueprintOwned || copy.recipe.blueprintSource === "market") ? "one_short" : "gather";
    if (copy.materials.some(material => material.component)) copy.state = "prepare_components";
    else if (copy.materials.some(material => material.pendingQuantity || material.plannedQuantity)) copy.state = "waiting_components";
    return copy;
  }
  const seeds = [braton,rhino,coda,hek,boltor,paris,ignis,soma,jat];
  const savedRefs = scenario === "empty" ? [] : refs ?? [braton.gameRef,boltor.gameRef,hek.gameRef,ignis.gameRef,paris.gameRef,jat.gameRef];
  const budget = freshBudget();let balance = credits;
  const queue = savedRefs.flatMap(ref => {
    const seed = seeds.find(item => item.gameRef === ref);if (!seed) return [];
    const row = evaluate(seed,budget,balance);
    balance = Math.max(0,balance-(row.totalCredits ?? 0));
    return [row];
  });
  const planDemand = new Map<string,CraftingIngredient>();
  for (const item of queue) for (const row of item.materials) {
    const rows = row.component ? row.component.materials : [row];
    for (const ingredient of rows) {
      const definition = ingredient.definition;const existing = planDemand.get(definition.gameRef);
      planDemand.set(definition.gameRef,{...definition,quantity:(existing?.quantity ?? 0)+definition.quantity});
    }
  }
  const knownPlanCredits = queue.reduce((sum,item) => sum+item.knownCredits,0);
  const totalPlanCredits = queue.some(item => item.totalCredits === null) ? null : knownPlanCredits;
  const totalBudget = freshBudget();
  const planMaterials = [...planDemand.values()].map(definition => evaluateMaterial(definition,totalBudget));
  const view: MasteryPlanView = {inventoryChecksum:"demo-mastery-plan",inventoryAvailable:true,historyAvailable:true,recipesAvailable:true,refreshFailed:false,
    foundryAvailable:scenario !== "no_foundry",foundryIssue:scenario === "foundry_issue",
    observedAt:new Date().toISOString(),metadataAt:"2026-10-02T01:00:00Z",credits,accountRank:12,savedRefs,
    candidates:seeds.map(item => queue.find(row => row.gameRef === item.gameRef) ?? evaluate(item,freshBudget(),credits)),queue,
    knownPlanCredits,totalPlanCredits,missingPlanCredits:totalPlanCredits === null ? null : Math.max(0,totalPlanCredits-credits),planMaterials};
  const priority: Record<MasteryPlanState,number> = {owned:0,ready_to_claim:1,crafting:2,craft:3,buy_blueprint:4,waiting_blueprint:5,waiting_components:6,prepare_components:7,one_short:8,gather:9,rank_locked:10,access_unknown:11,no_recipe:12,unknown:13,mastered:13};
  view.candidates.sort((a,b) => priority[a.state]-priority[b.state] || a.missingTypes-b.missingTypes || (a.totalCredits ?? Infinity)-(b.totalCredits ?? Infinity) || a.displayName.localeCompare(b.displayName,"ru"));
  if (scenario === "none") { view.historyAvailable = false;view.inventoryAvailable = false;view.observedAt = null; }
  if (scenario === "catalog") { view.recipesAvailable = false;view.candidates = view.candidates.filter(item => item.ownedQuantity); }
  if (scenario === "stale") view.refreshFailed = true;
  return view;
}
