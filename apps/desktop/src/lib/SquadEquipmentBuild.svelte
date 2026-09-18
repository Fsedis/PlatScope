<script lang="ts">
  import PersonalGoalImage from "./PersonalGoalImage.svelte";
  import SquadUpgradeCard from "./SquadUpgradeCard.svelte";
  import SquadRiven from "./SquadRiven.svelte";
  import { partName, refinementName, shardColor, type SquadEquipment, type SquadPart } from "./squad";

  export let equipment: SquadEquipment;
  export let player: string;
  export let capturedAt: string | null | undefined;
  $: slots = equipment.upgradeSlots;
  $: mods = equipment.upgrades.filter(part => part.kind === "mod");
  $: arcaneSlots = (slots ?? []).filter(slot => slot.part?.kind === "arcane");
  $: arcanes = equipment.upgrades.filter(part => part.kind === "arcane" && !arcaneSlots.some(slot => slot.part?.path === part.path && slot.part.rank === part.rank));
  $: other = equipment.upgrades.filter(part => !["mod", "arcane"].includes(part.kind));
  $: date = capturedAt ? new Date(capturedAt).toLocaleString("ru-RU") : "неизвестно";
  $: sourceParts = [...new Map([
    equipment.item, ...equipment.upgrades, ...(slots ?? []).flatMap(slot => slot.part ? [slot.part] : []),
    ...equipment.modularParts, ...(equipment.shards ?? []).map(shard => shard.effect),
    equipment.abilityOverride?.ability, equipment.context?.focus, equipment.context?.relic,
  ].filter((part): part is SquadPart => Boolean(part)).map(part => [part.path, part])).values()];
</script>

<article class="equipment-build" aria-label={`Билд: ${partName(equipment.item)}`}>
  <header class="equipment-hero">
    <div class="hero-art"><PersonalGoalImage src={equipment.item.imageUrl ?? null} size="large" /></div>
    <div class="hero-copy">
      <p class="eyebrow">{equipment.category}{equipment.configuration ? ` · Конфигурация ${equipment.configuration}` : ""}</p>
      <h2>{partName(equipment.item)}</h2>
      {#if equipment.item.nameEn && equipment.item.nameEn !== equipment.item.name}<p class="english" lang="en">{equipment.item.nameEn}</p>{/if}
      <div class="facts">
        {#if equipment.level !== null}<span>Уровень <strong>{equipment.level}</strong></span>{/if}
        {#if equipment.forma !== null}<span>Формы <strong>{equipment.forma}</strong></span>{/if}
      </div>
    </div>
    <div class="data-state"><span>Неполные данные</span><small>{equipment.source === "memoryConfiguration" ? "Владелец и актуальность не подтверждены" : "Принадлежность требует проверки"}</small></div>
  </header>

  {#if equipment.category === "Мод разлома"}
    <section class="build-section">
      <h3>Свойства разлома</h3>
      {#if equipment.item.fingerprint}<SquadRiven part={equipment.item} />{:else}<p class="hint">Свойства и число преобразований не получены.</p>{/if}
    </section>
  {:else}
    <section class="build-section">
      <div class="section-heading"><h3>{slots ? "Улучшения по порядку" : "Моды"}</h3><span class="count">{slots ? slots.filter(slot => slot.part?.kind !== "arcane").length : mods.length}</span></div>
      {#if slots}<p class="hint order-hint">Позиции из записи игры, не схема слотов Арсенала.</p>{/if}
      {#if slots ? slots.some(slot => slot.part?.kind !== "arcane") : mods.length}
        <ul class="mod-grid">
          {#if slots}
            {#each slots.filter(slot => slot.part?.kind !== "arcane") as slot}
              <li><SquadUpgradeCard part={slot.part} status={slot.status} position={slot.index + 1} /></li>
            {/each}
          {:else}
            {#each mods as part}<li><SquadUpgradeCard {part} /></li>{/each}
          {/if}
        </ul>
      {:else}<p class="section-empty">{slots?.length === 0 ? "В полученной конфигурации список улучшений пуст." : "Моды не определены в полученных данных."}</p>{/if}
    </section>

    <section class="build-section supporting">
      <div class="section-heading"><h3>Мистификаторы</h3><span class="count">{arcaneSlots.length + arcanes.length}</span></div>
      {#if arcaneSlots.length || arcanes.length}<ul class="compact-grid">
        {#each arcaneSlots as slot}<li><SquadUpgradeCard part={slot.part} status={slot.status} position={slot.index + 1} compact /></li>{/each}
        {#each arcanes as part}<li><SquadUpgradeCard {part} compact /></li>{/each}
      </ul>{:else}<p class="hint">Не определены в полученных данных.</p>{/if}
    </section>
  {/if}

  {#if equipment.category === "Варфрейм"}
    <div class="frame-extras">
      <section class="build-section supporting">
        <div class="section-heading"><h3>Осколки Архонта</h3>{#if equipment.shards != null}<span class="count">{equipment.shards.length}</span>{/if}</div>
        {#if equipment.shards == null}<p class="hint">Данные не получены. Наличие осколков неизвестно.</p>
        {:else if !equipment.shards.length}<p class="hint">В полученном снимке осколков нет.</p>
        {:else}<ul class="shards">
          {#each equipment.shards as shard}
            <li class="shard" data-color={shard.color.replace(/_MYTHIC$/, "")}>
              <span class="crystal" class:tau={shard.color.endsWith("_MYTHIC")} aria-hidden="true"></span>
              <div><strong>{shardColor(shard.color)}</strong><p>{partName(shard.effect)}</p>{#if shard.effect.nameEn && shard.effect.nameEn !== shard.effect.name}<small lang="en">{shard.effect.nameEn}</small>{/if}</div>
            </li>
          {/each}
        </ul>{/if}
      </section>
      <section class="build-section supporting">
        <h3>Заменённая способность</h3>
        {#if equipment.abilityOverride}
          <div class="part-row"><PersonalGoalImage src={equipment.abilityOverride.ability.imageUrl ?? null} /><div><strong>{partName(equipment.abilityOverride.ability)}</strong>{#if equipment.abilityOverride.ability.nameEn && equipment.abilityOverride.ability.nameEn !== equipment.abilityOverride.ability.name}<small lang="en">{equipment.abilityOverride.ability.nameEn}</small>{/if}<p class="hint">{equipment.abilityOverride.slot ? `Вместо способности ${equipment.abilityOverride.slot}` : "Номер способности неизвестен"}</p></div></div>
        {:else}<p class="hint">Не указана в полученных данных.</p>{/if}
      </section>
    </div>
  {/if}

  {#if equipment.modularParts.length}
    <section class="build-section supporting"><h3>Состав предмета</h3><ul class="compact-grid">{#each equipment.modularParts as part}<li class="part-row"><PersonalGoalImage src={part.imageUrl ?? null} /><div><strong>{partName(part)}</strong>{#if part.nameEn && part.nameEn !== part.name}<small lang="en">{part.nameEn}</small>{/if}</div></li>{/each}</ul></section>
  {/if}
  {#if equipment.context?.focus || equipment.context?.relic}
    <div class="context">
      {#if equipment.context.focus}<div><span>Фокус</span><strong>{partName(equipment.context.focus)}</strong>{#if equipment.context.focus.nameEn !== equipment.context.focus.name}<small lang="en">{equipment.context.focus.nameEn}</small>{/if}</div>{/if}
      {#if equipment.context.relic}<div><span>Реликвия · {refinementName(equipment.context.refinement)}</span><strong>{partName(equipment.context.relic)}</strong>{#if equipment.context.relic.nameEn !== equipment.context.relic.name}<small lang="en">{equipment.context.relic.nameEn}</small>{/if}</div>{/if}
    </div>
  {/if}

  <details class="data-details">
    <summary>Подробности данных{other.length || equipment.unreadableUpgrades ? " · есть неопределённые элементы" : ""}</summary>
    <div class="data-body">
      <p>{player} · Снимок: {date}.</p>
      <p>{equipment.source === "memoryConfiguration" ? "Конфигурация из памяти. Владелец, активный вариант и актуальность не подтверждены; записи могут относиться к старым состояниям предмета." : "Участники определяются по журналу игры, экипировка читается из памяти. Принадлежность сопоставлена по размеру данных — проверьте её перед использованием."}</p>
      <p>{equipment.inventoryResolved ? "Улучшения сопоставлены внутри одного инвентаря. Порядок следует записи игры, а не сетке Арсенала." : "Ранги, порядок и параметры могут быть неполными. Неизвестные значения не означают ноль или максимальный ранг."}</p>
      <p>Уровень: {equipment.level ?? "неизвестен"} · Формы: {equipment.forma ?? "неизвестно"}. {equipment.category === "Варфрейм" ? "Числовые бонусы осколков не расшифрованы. Замену способности сверяйте с Арсеналом." : ""}</p>
      {#if equipment.unreadableUpgrades}<p>Не удалось разобрать элементов: {equipment.unreadableUpgrades}.</p>{/if}
      {#if other.length}<h4>Другие элементы</h4><p>Могут включать косметику; они не считаются модами.</p><ul>{#each other as part}<li><strong>{partName(part)}</strong>{#if part.nameEn && part.nameEn !== part.name}<small lang="en">{part.nameEn}</small>{/if}<code>{part.path}</code></li>{/each}</ul>{/if}
      <h4>Исходные пути предметов</h4>
      <ul>{#each sourceParts as part}<li><span>{partName(part)}</span><code>{part.path}</code></li>{/each}</ul>
      <p>Сохранённая коллекция хранится на этом компьютере. Применяйте состав вручную в Арсенале.</p>
    </div>
  </details>
</article>

<style>
  .equipment-build { display:grid; gap:1.25rem; min-width:0; }
  .equipment-hero { display:flex; gap:1rem; align-items:center; min-width:0; padding:1rem 1.15rem; border:1px solid var(--border); border-radius:.8rem; background:linear-gradient(110deg,var(--surface-2),var(--surface-1) 65%); }
  .hero-art { padding:.25rem; border:1px solid var(--border); border-radius:.7rem; background:var(--surface-1); }
  .hero-copy { min-width:0; flex:1; } h2 { margin:.2rem 0; font-size:clamp(1.2rem,1.7vw,1.8rem); line-height:1.2; overflow-wrap:anywhere; } p { margin:0; }
  .eyebrow { color:var(--accent-strong); font-size:.7rem; font-weight:700; letter-spacing:.07em; text-transform:uppercase; }
  .english,.hint,small { color:var(--text-muted); font-size:.78rem; line-height:1.5; } .english { font-size:.88rem; } small { display:block; }
  .facts { display:flex; flex-wrap:wrap; gap:.6rem; margin-top:.65rem; font-size:.76rem; } .facts span { padding:.25rem .5rem; background:var(--surface-1); border:1px solid var(--border); border-radius:.3rem; color:var(--text-muted); } .facts strong { margin-left:.25rem; color:var(--text); }
  .data-state { align-self:flex-start; display:grid; justify-items:end; gap:.35rem; max-width:12rem; text-align:right; } .data-state>span { padding:.25rem .55rem; border:1px solid var(--border); border-radius:2rem; font-size:.69rem; color:var(--accent-strong); background:var(--surface-1); } .data-state small { font-size:.68rem; }
  .build-section { min-width:0; } h3 { margin:0; font-size:.96rem; } h4 { margin:.5rem 0 0; font-size:.8rem; } .build-section>h3 { margin-bottom:.8rem; } .section-heading { display:flex; align-items:center; gap:.5rem; margin-bottom:.8rem; }
  .count { border:1px solid var(--border); padding:.05rem .38rem; border-radius:.35rem; color:var(--text-muted); font-size:.7rem; font-variant-numeric:tabular-nums; }
  ul { list-style:none; margin:0; padding:0; } li { min-width:0; } .mod-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(145px,1fr)); gap:.7rem; align-items:stretch; } .mod-grid>li { display:flex; }
  .order-hint { margin:-.4rem 0 .7rem; } .section-empty { padding:1.2rem; border:1px dashed var(--border); border-radius:.6rem; color:var(--text-muted); font-size:.83rem; }
  .supporting { border:1px solid var(--border); border-radius:.7rem; background:var(--surface-2); padding:1rem; }
  .compact-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr)); gap:.65rem; }
  .frame-extras { display:grid; grid-template-columns:minmax(0,1.35fr) minmax(0,1fr); gap:.8rem; }
  .shards { display:grid; gap:.55rem; } .shard { display:flex; align-items:center; gap:.7rem; padding:.55rem .6rem; border-radius:.4rem; background:var(--surface-1); --shard-color:var(--text-muted); } .shard strong { font-size:.76rem; } .shard p { margin:.1rem 0; font-size:.8rem; } .shard small { font-size:.7rem; }
  .shard[data-color="ACC_RED"] { --shard-color:#ac4648; } .shard[data-color="ACC_BLUE"] { --shard-color:#4774a0; } .shard[data-color="ACC_YELLOW"] { --shard-color:#af8229; } .shard[data-color="ACC_GREEN"] { --shard-color:#568466; } .shard[data-color="ACC_ORANGE"] { --shard-color:#b06a35; } .shard[data-color="ACC_PURPLE"] { --shard-color:#8b65a0; }
  .crystal { flex:none; width:17px; height:30px; background:linear-gradient(115deg,color-mix(in srgb,var(--shard-color),white 35%) 48%,var(--shard-color) 50%); clip-path:polygon(50% 0,100% 30%,75% 85%,50% 100%,20% 80%,0 30%); } .crystal.tau { filter:drop-shadow(0 0 3px var(--shard-color)); background:linear-gradient(115deg,color-mix(in srgb,var(--shard-color),white 60%) 48%,var(--shard-color) 50%); }
  .part-row { display:flex; align-items:center; gap:.7rem; min-width:0; overflow-wrap:anywhere; } .part-row>div { min-width:0; } .part-row strong { font-size:.83rem; } .part-row small { font-size:.73rem; }
  .context { display:flex; flex-wrap:wrap; gap:1rem 2rem; padding:.15rem .2rem; } .context>div { min-width:0; overflow-wrap:anywhere; } .context span { display:block; font-size:.7rem; color:var(--text-muted); margin-bottom:.2rem; } .context strong { font-size:.85rem; }
  .data-details { border-top:1px solid var(--border); padding-top:1rem; color:var(--text-muted); font-size:.78rem; } summary { cursor:pointer; font-weight:600; } .data-body { display:grid; gap:.7rem; padding-top:1rem; line-height:1.6; overflow-wrap:anywhere; } .data-body ul { display:grid; gap:.55rem; } code { display:block; font-size:.7rem; overflow-wrap:anywhere; } summary:focus-visible { outline:2px solid var(--accent); outline-offset:3px; }
  @media (max-width:1100px) { .mod-grid { grid-template-columns:repeat(auto-fill,minmax(130px,1fr)); } .equipment-hero { flex-wrap:wrap; } .data-state { flex-basis:100%; max-width:none; justify-items:start; text-align:left; display:flex; align-items:center; flex-wrap:wrap; } .frame-extras { grid-template-columns:1fr; } }
  @media (max-width:540px) { .equipment-hero { padding:.75rem; gap:.75rem; } .hero-copy { flex-basis:calc(100% - 132px); } .mod-grid { grid-template-columns:repeat(2,minmax(0,1fr)); gap:.5rem; } .supporting { padding:.75rem; } }
</style>
