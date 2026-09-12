<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { DbwinCaptureStatus, DbwinEntry } from "./dbwinCapture";

  let status: DbwinCaptureStatus | null = null;
  let entries: DbwinEntry[] = [];
  let busy = false;
  let polling = false;
  let disposed = false;
  let revision = 0;
  let error = "";
  let loadError = "";
  let mark = "";
  let search = "";
  let paused = false;
  $: visibleEntries = entries.filter(entry => entry.message.toLocaleLowerCase("ru").includes(search.toLocaleLowerCase("ru")));

  function accept(next: DbwinCaptureStatus): void {
    status = next;
    if (!paused) entries = [...next.entries].reverse();
  }

  async function refresh(): Promise<void> {
    if (busy || polling || disposed) return;
    polling = true;
    const current = revision;
    try {
      const next = await invoke<DbwinCaptureStatus>("dbwin_capture_status");
      if (!disposed && current === revision) { accept(next); loadError = ""; }
    } catch (reason) {
      if (!disposed && current === revision) loadError = `Не удалось получить состояние записи: ${String(reason)}`;
    } finally { polling = false; }
  }

  async function act(command: string): Promise<void> {
    if (busy) return;
    busy = true;
    revision += 1;
    error = "";
    try {
      const next = await invoke<DbwinCaptureStatus>(command, command === "mark_dbwin_capture" ? { message: mark } : {});
      if (disposed) return;
      if (command === "start_dbwin_capture") { paused = false; search = ""; }
      accept(next);
      if (command === "mark_dbwin_capture") mark = "";
    } catch (reason) {
      if (!disposed) error = String(reason);
    } finally { busy = false; }
  }

  async function openFolder(): Promise<void> {
    try { await invoke("open_dbwin_capture_folder"); }
    catch (reason) { error = `Не удалось открыть папку: ${String(reason)}`; }
  }

  function togglePreview(): void {
    paused = !paused;
    if (!paused && status) entries = [...status.entries].reverse();
  }

  function elapsed(ms: number): string {
    const seconds = Math.floor(ms / 1000);
    return `${Math.floor(seconds / 60).toString().padStart(2, "0")}:${(seconds % 60).toString().padStart(2, "0")}.${(ms % 1000).toString().padStart(3, "0")}`;
  }

  onMount(() => {
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 1000);
    return () => { disposed = true; clearInterval(timer); };
  });
</script>

<section class="capture" aria-labelledby="dbwin-heading">
  <div class="heading">
    <div>
      <h2 id="dbwin-heading">Сообщения Warframe</h2>
      <p>Запишите сообщения игры через DBWIN, чтобы изучить события миссии, наград и других действий.</p>
    </div>
    {#if status?.active}<span class="recording">● Идёт запись</span>{/if}
  </div>
  <p class="note">Журнал сохраняется только на этом компьютере. В нём могут быть ники и другие личные данные. В обычный отчёт диагностики он не входит.</p>

  <div class="actions">
    {#if status?.active}
      <button type="button" onclick={() => act("stop_dbwin_capture")} disabled={busy}>Остановить запись</button>
    {:else}
      <button type="button" onclick={() => act("start_dbwin_capture")} disabled={busy || !status?.connected || !status?.supported}>{busy ? "Начинаем запись…" : "Начать запись"}</button>
    {/if}
    <button type="button" class="secondary" onclick={openFolder}>Открыть папку журналов</button>
  </div>

  <p class="state" role="status">
    {#if !status}Проверяем слушатель игры…
    {:else if !status.connected}Слушатель игры недоступен. Если состояние не меняется, перезапустите PlatScope.
    {:else if !status.supported}Для записи нужен обновлённый OCR-модуль. Перезапустите обновлённый PlatScope.
    {:else if status.active && status.messages === 0}Запись началась. Ожидаем сообщения Warframe — запустите игру или выполните действие в ней.
    {:else if status.active}Получено сообщений: {status.messages.toLocaleString("ru")}. Отметок: {status.markers}. Сохранено: {(status.bytes / 1024).toLocaleString("ru", { maximumFractionDigits: 1 })} КиБ.
    {:else if status.path}{status.stopReason ?? "Запись завершена."} Сообщений: {status.messages.toLocaleString("ru")}. Отметок: {status.markers}.
    {:else}Готово к записи. Одна сессия — до 2 часов или 100 МиБ.
    {/if}
  </p>
  {#if status?.sharedListener}<p class="warning">Обнаружен другой слушатель DBWIN. Он может перехватывать часть сообщений; для полной записи закройте другие программы просмотра отладочного потока.</p>{/if}
  {#if error || loadError || status?.error}<p class="failure" role="alert">{error || loadError || status?.error}</p>{/if}
  {#if status?.path}<p class="path">Файл сессии: <code>{status.path}</code></p>{/if}

  {#if status?.active}
    <form class="marker" onsubmit={(event) => { event.preventDefault(); void act("mark_dbwin_capture"); }}>
      <label for="dbwin-marker">Отметка о действии в игре</label>
      <div class="actions">
        <input id="dbwin-marker" bind:value={mark} maxlength="500" placeholder="Например: открыл инвентарь" disabled={busy} />
        <button type="submit" class="secondary" disabled={busy || !mark.trim()}>Добавить отметку</button>
      </div>
    </form>
    <p class="note">Можно закрыть диагностику и играть — запись продолжится. При закрытии PlatScope она закончится.</p>
  {/if}

  {#if status?.path}
    <div class="preview-heading">
      <h3>Последние сообщения</h3>
      <button type="button" class="secondary" onclick={togglePreview}>{paused ? "Продолжить просмотр" : "Приостановить просмотр"}</button>
    </div>
    <label for="dbwin-search">Поиск по последним 100 записям</label>
    <input id="dbwin-search" type="search" bind:value={search} placeholder="Например: reward, inventory, error" />
    <p class="note">Новые записи сверху. {paused ? "Просмотр приостановлен; сохранение файла продолжается, пока идёт запись." : "Просмотр обновляется раз в секунду."} Полный текст всех принятых сообщений — в файле JSONL.</p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Область прокручивается с клавиатуры.) -->
    <div class="entries" tabindex="0" role="region" aria-label="Предпросмотр сообщений Warframe">
      {#each visibleEntries as entry (entry.sequence)}
        <article class:marker-entry={entry.kind === "marker"}>
          <div class="entry-meta"><span>#{entry.sequence} · +{elapsed(entry.elapsedMs)}</span><span>{entry.kind === "marker" ? "Твоя отметка" : entry.kind === "message" ? `Warframe · процесс ${entry.processId}` : "Сессия"}</span></div>
          <pre>{entry.message}</pre>
        </article>
      {:else}<p class="empty">{search ? "В последних записях совпадений нет. Полную сессию можно найти в файле." : "Сообщения ещё не получены."}</p>{/each}
    </div>
  {/if}
</section>

<style>
  .capture { min-width: 0; padding: 1rem; border: 1px solid var(--border); border-radius: .75rem; background: var(--surface-1); }
  .heading, .preview-heading, .actions, .entry-meta { display: flex; flex-wrap: wrap; align-items: center; gap: .65rem 1rem; }
  .heading, .preview-heading, .entry-meta { justify-content: space-between; }
  .heading > div { flex: 1; min-width: 15rem; }
  h2 { margin: 0 0 .35rem; font-size: 1.2rem; }
  h3 { margin: 0; font-size: 1rem; }
  p { margin: .55rem 0; line-height: 1.5; }
  .heading p, .note, .entry-meta { color: var(--text-muted); }
  .note { font-size: .82rem; }
  .actions { margin: .85rem 0; }
  button { min-height: 2.35rem; }
  button.secondary { background: transparent; }
  input { width: 100%; min-width: 0; min-height: 2.35rem; box-sizing: border-box; }
  .marker .actions input { flex: 1 1 16rem; }
  label { display: block; margin-bottom: .35rem; font-size: .875rem; font-weight: 650; }
  .state { font-size: .9rem; }
  .recording { color: var(--accent-strong); font-weight: 700; }
  .warning, .failure { border: 1px solid var(--border); padding: .65rem; border-radius: .4rem; }
  .failure { color: var(--danger); border-color: var(--danger); }
  .path { font-size: .82rem; overflow-wrap: anywhere; }
  .preview-heading { margin: 1.1rem 0 .75rem; }
  .entries { margin-top: .75rem; max-height: 24rem; overflow: auto; border: 1px solid var(--border); border-radius: .5rem; background: var(--app-bg); }
  article { padding: .65rem .8rem; border-bottom: 1px solid var(--border); }
  article:last-child { border-bottom: 0; }
  .marker-entry { background: var(--surface-3); border-left: 3px solid var(--accent-strong); }
  .entry-meta { font-size: .72rem; margin-bottom: .35rem; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; margin: 0; font-size: .8rem; line-height: 1.5; font-family: ui-monospace, Consolas, monospace; }
  .empty { padding: .75rem; color: var(--text-muted); }
</style>
