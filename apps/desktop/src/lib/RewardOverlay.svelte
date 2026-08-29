<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  import { formatPlatinum } from "./market";
  import {
    confidencePercent,
    overlayContentScale,
    rewardPrice,
    type RelicRewardChoice,
    type RelicRewardScanView,
    type RewardSetPart,
  } from "./relicRewards";

  let result: RelicRewardScanView | null = null;
  let unavailable = false;

  function partStatus(reward: RelicRewardChoice, part: RewardSetPart): string {
    if (reward.set?.readyComponents === null) return "Количество неизвестно";
    if (part.requiredQuantity > 1) {
      return `Есть ${part.ownedQuantity} из ${part.requiredQuantity}`;
    }
    return part.ownedQuantity > 0 ? `Есть ×${part.ownedQuantity}` : "Нет";
  }

  function partFallback(name: string): string {
    return name.slice(0, 2).toLocaleUpperCase("en");
  }

  onMount(() => {
    document.documentElement.classList.add("overlay-mode");
    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    void invoke<RelicRewardScanView | null>("latest_relic_rewards")
      .then((view) => {
        if (!disposed) result = view;
      })
      .catch(() => {
        if (!disposed) unavailable = true;
      });

    void listen<RelicRewardScanView>("relic-rewards-updated", (event) => {
      result = event.payload;
      unavailable = false;
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });

    return () => {
      disposed = true;
      unlisten?.();
      document.documentElement.classList.remove("overlay-mode");
    };
  });
</script>

<main class="overlay-shell" aria-label="Подсказка выбора награды">
  {#if result?.status === "ok" && result.rewards.length >= 2}
    {@const contentScale = overlayContentScale(result.overlayScale, window.devicePixelRatio)}
    <section
      class="overlay-grid"
      aria-label="Распознанные награды"
      style={`grid-template-columns: repeat(${result.rewards.length}, minmax(0, 1fr)); width: ${100 / contentScale}%; height: ${100 / contentScale}%; transform: scale(${contentScale});`}
    >
      {#each result.rewards as reward, index (reward.slot)}
        {@const price = rewardPrice(reward)}
        <article class:recommended={reward.recommended} class:uncertain={reward.confidence < 0.75}>
          <header class="card-topline">
            <span>Вариант {index + 1}</span>
            {#if !reward.itemId}
              <strong class="recognition-warning">Не распознано</strong>
            {:else if reward.recommended}
              <strong>Лучший выбор</strong>
            {/if}
          </header>

          <div class="reward-main">
            <div class="reward-art" aria-hidden="true">
              {#if reward.market?.imageUrl}
                <img src={reward.market.imageUrl} alt="" />
              {:else}
                <span>◇</span>
              {/if}
            </div>
            <div class="reward-copy">
              <h2>{reward.displayName ?? "Не распознано"}</h2>
              <strong class="reward-price">{price === null ? "—" : formatPlatinum(price, "ru")}</strong>
            </div>
          </div>

          <dl class="reward-facts">
            <div><dt>Дукаты</dt><dd>{reward.ducats ?? "—"}</dd></div>
            <div><dt>У вас</dt><dd>{reward.ownedQuantity === null ? "—" : `×${reward.ownedQuantity}`}</dd></div>
          </dl>

          {#if reward.set}
            <section class:set-complete={reward.completesSet} class="set-card" aria-label="Прогресс комплекта">
              <header class="set-heading">
                <div class="set-identity">
                  <span>Полный комплект</span>
                  <strong>{reward.set.setName}</strong>
                </div>
                <div class="set-price">
                  <span>Цена сета</span>
                  <strong>{reward.set.setPrice === null ? "—" : formatPlatinum(reward.set.setPrice, "ru")}</strong>
                </div>
              </header>

              <div class="set-summary">
                {#if reward.set.readyComponents === null}
                  Инвентарь не загружен
                {:else}
                  Собрано <strong>{reward.set.readyComponents} из {reward.set.totalComponents}</strong> частей
                {/if}
              </div>

              <div class="parts-heading">Состав комплекта</div>
              {#if reward.set.parts.length}
                <div class="set-parts">
                  {#each reward.set.parts as part (part.name)}
                    <div
                      class="part-chip"
                      class:missing={reward.set.readyComponents !== null && part.ownedQuantity < part.requiredQuantity}
                      class:inventory-unknown={reward.set.readyComponents === null}
                      class:current-reward={part.isReward}
                      aria-label={`${part.name}: ${partStatus(reward, part)}${part.isReward ? ", эта награда" : ""}`}
                    >
                      <div class="part-art" aria-hidden="true">
                        {#if part.imageUrl}<img src={part.imageUrl} alt="" />{:else}<span>{partFallback(part.name)}</span>{/if}
                      </div>
                      <div class="part-copy">
                        <strong>{part.name}</strong>
                        <span>{partStatus(reward, part)}</span>
                      </div>
                      {#if part.isReward}<small>Эта награда</small>{/if}
                    </div>
                  {/each}
                </div>
              {:else}
                <p class="no-parts">Состав комплекта не найден</p>
              {/if}
            </section>
          {:else}
            <div class="simple-status">
              {reward.confidence < 0.75
                ? `Проверьте название · OCR ${confidencePercent(reward.confidence)}%`
                : "Рыночная цена детали"}
            </div>
          {/if}
        </article>
      {/each}
    </section>
  {:else}
    <section class="overlay-empty" aria-live="polite">
      <strong>{unavailable ? "Оверлей временно недоступен" : "Распознаём награды…"}</strong>
    </section>
  {/if}
</main>

<style>
  .overlay-shell {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: transparent;
    color: var(--text);
    user-select: none;
  }

  .overlay-grid {
    display: grid;
    grid-auto-rows: max-content;
    align-items: start;
    gap: 4px;
    transform-origin: left top;
  }

  article {
    position: relative;
    display: grid;
    grid-template-rows: auto auto auto auto;
    align-self: start;
    gap: 0.55rem;
    min-width: 0;
    padding: 0.72rem;
    overflow: hidden;
    border: 1px solid oklch(0.64 0.045 66 / 0.72);
    border-radius: 0.78rem;
    background: oklch(0.965 0.022 80 / 0.97);
    box-shadow:
      0 1px 2px oklch(0.25 0.025 55 / 0.16),
      0 10px 24px oklch(0.25 0.025 55 / 0.16),
      inset 0 1px 0 oklch(1 0 0 / 0.55);
    -webkit-backdrop-filter: blur(12px) saturate(108%);
    backdrop-filter: blur(12px) saturate(108%);
  }

  article.recommended {
    border-color: oklch(0.57 0.11 72 / 0.95);
    background: oklch(0.95 0.038 78 / 0.98);
    box-shadow:
      inset 0 3px 0 var(--gold),
      0 1px 2px oklch(0.25 0.025 55 / 0.18),
      0 10px 24px oklch(0.25 0.025 55 / 0.17);
  }

  article.uncertain:not(.recommended) {
    border-color: oklch(0.68 0.085 32 / 0.72);
  }

  .card-topline,
  .set-heading,
  .reward-main,
  .reward-facts,
  .reward-facts div,
  .part-chip {
    display: flex;
    align-items: center;
  }

  .card-topline {
    justify-content: space-between;
    gap: 0.55rem;
    color: var(--text-muted);
    font-size: 0.78rem;
    font-weight: 750;
    letter-spacing: 0.055em;
    line-height: 1.1;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .card-topline strong {
    border-radius: 999px;
    padding: 0.22rem 0.48rem;
    background: var(--accent-strong);
    color: var(--surface-1);
    font-size: 0.7rem;
    letter-spacing: 0.02em;
  }

  .card-topline strong.recognition-warning {
    background: var(--danger-strong, oklch(0.49 0.15 28));
  }

  .reward-main {
    align-items: stretch;
    gap: 0.72rem;
    min-width: 0;
  }

  .reward-art {
    display: grid;
    flex: 0 0 4.9rem;
    width: 4.9rem;
    height: 4.9rem;
    place-items: center;
    overflow: hidden;
    border-radius: 0.58rem;
    background: oklch(0.99 0.012 82 / 0.9);
    outline: 1px solid oklch(0 0 0 / 0.1);
  }

  .reward-art img,
  .part-art img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .reward-art > span {
    color: var(--text-subtle);
    font-size: 1.75rem;
  }

  .reward-copy {
    display: grid;
    flex: 1 1 auto;
    align-content: space-between;
    min-width: 0;
  }

  h2 {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 0.96rem;
    font-weight: 720;
    letter-spacing: -0.005em;
    line-height: 1.2;
    text-wrap: balance;
    overflow-wrap: break-word;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .reward-price {
    color: var(--accent-strong);
    font-size: 2rem;
    font-weight: 780;
    letter-spacing: -0.04em;
    line-height: 0.95;
    font-variant-numeric: tabular-nums;
  }

  .reward-facts {
    gap: 0.42rem;
    margin: 0;
  }

  .reward-facts div {
    flex: 1 1 0;
    justify-content: space-between;
    gap: 0.4rem;
    min-width: 0;
    border-radius: 0.48rem;
    padding: 0.42rem 0.52rem;
    background: oklch(0.91 0.025 78 / 0.88);
  }

  .reward-facts dt,
  .reward-facts dd {
    margin: 0;
    font-size: 0.81rem;
    line-height: 1;
    white-space: nowrap;
  }

  .reward-facts dt {
    color: var(--text-muted);
  }

  .reward-facts dd {
    color: var(--text);
    font-weight: 780;
    font-variant-numeric: tabular-nums;
  }

  .set-card {
    position: relative;
    display: grid;
    grid-template-rows: auto auto auto auto;
    gap: 0.38rem;
    align-self: start;
    min-width: 0;
    overflow: hidden;
    border-radius: 0.58rem;
    padding: 0.58rem;
    background: oklch(0.92 0.034 72 / 0.92);
  }

  .set-card.set-complete {
    background: var(--success-soft);
  }

  .set-heading {
    justify-content: space-between;
    gap: 0.6rem;
    min-width: 0;
  }

  .set-identity,
  .set-price {
    display: grid;
    min-width: 0;
  }

  .set-heading span,
  .parts-heading,
  .no-parts {
    color: var(--text-muted);
    font-size: 0.72rem;
    font-weight: 680;
    line-height: 1.15;
  }

  .set-identity strong {
    overflow: hidden;
    color: var(--text);
    font-size: 0.86rem;
    font-weight: 760;
    line-height: 1.22;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .set-price {
    flex: 0 0 auto;
    justify-items: end;
  }

  .set-price strong {
    color: var(--accent-strong);
    font-size: 1.18rem;
    font-weight: 800;
    line-height: 1.05;
    font-variant-numeric: tabular-nums;
  }

  .set-summary {
    border-radius: 0.38rem;
    padding: 0.34rem 0.45rem;
    background: oklch(0.975 0.014 80 / 0.72);
    color: var(--text-muted);
    font-size: 0.77rem;
    line-height: 1.15;
  }

  .set-summary strong {
    color: var(--text);
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .parts-heading {
    text-transform: uppercase;
    letter-spacing: 0.045em;
  }

  .set-parts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    align-content: start;
    gap: 0.32rem;
  }

  .set-parts > .part-chip:last-child:nth-child(odd) {
    grid-column: 1 / -1;
  }

  .part-chip {
    position: relative;
    gap: 0.38rem;
    min-width: 0;
    border: 1px solid transparent;
    border-radius: 0.46rem;
    padding: 0.28rem 0.36rem;
    background: oklch(0.975 0.014 80 / 0.86);
  }

  .part-chip.missing {
    background: oklch(0.88 0.012 76 / 0.78);
  }

  .part-chip.missing .part-art,
  .part-chip.inventory-unknown .part-art {
    filter: grayscale(1);
    opacity: 0.52;
  }

  .part-chip.missing .part-copy strong,
  .part-chip.missing .part-copy span,
  .part-chip.inventory-unknown .part-copy {
    color: var(--text-subtle);
  }

  .part-chip.current-reward {
    border-color: oklch(0.57 0.11 72 / 0.88);
    background: oklch(0.965 0.045 82 / 0.94);
    box-shadow: inset 3px 0 0 var(--gold);
  }

  .part-art {
    display: grid;
    flex: 0 0 2.2rem;
    width: 2.2rem;
    height: 2.2rem;
    place-items: center;
    overflow: hidden;
    border-radius: 0.34rem;
    background: oklch(1 0 0 / 0.62);
    outline: 1px solid oklch(0 0 0 / 0.1);
  }

  .part-art span {
    color: var(--text-muted);
    font-size: 0.58rem;
    font-weight: 750;
  }

  .part-copy {
    display: grid;
    min-width: 0;
  }

  .part-copy strong {
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 0.75rem;
    font-weight: 740;
    line-height: 1.12;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .part-copy span {
    color: var(--success-strong, oklch(0.43 0.11 145));
    font-size: 0.7rem;
    font-weight: 720;
    line-height: 1.15;
    font-variant-numeric: tabular-nums;
  }

  .part-chip small {
    position: absolute;
    inset-block-start: 0.2rem;
    inset-inline-end: 0.25rem;
    border-radius: 999px;
    padding: 0.1rem 0.25rem;
    background: var(--accent-strong);
    color: var(--surface-1);
    font-size: 0.54rem;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
  }

  .part-chip.current-reward .part-copy {
    padding-block-start: 0.52rem;
  }

  .no-parts {
    margin: 0;
  }

  .simple-status {
    align-self: end;
    border-radius: 0.48rem;
    padding: 0.52rem 0.58rem;
    background: oklch(0.91 0.025 78 / 0.85);
    color: var(--text-muted);
    font-size: 0.8rem;
    font-weight: 680;
  }

  .overlay-empty {
    display: grid;
    width: 100%;
    height: 100%;
    place-items: center;
    border: 1px solid oklch(0.64 0.045 66 / 0.72);
    border-radius: 0.68rem;
    background: oklch(0.965 0.022 80 / 0.97);
    box-shadow: 0 10px 24px oklch(0.25 0.025 55 / 0.16);
  }

  .overlay-empty strong {
    color: var(--text-muted);
    font-size: 0.9rem;
  }
</style>
