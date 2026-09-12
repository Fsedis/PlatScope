<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  interface Recording { active: boolean; scanning: boolean; path: string | null; startedAt: string | null; samples: number; bytes: number; changedFields: string[]; error: string | null; stopReason: string | null }
  interface BinaryRecording { active: boolean; stopping: boolean; scanning: boolean; path: string | null; samples: number; bytes: number; readBytes: number; holes: number; lastComplete: boolean | null; error: string | null; stopReason: string | null }
  let normal: Recording | null = null;
  let binary: BinaryRecording | null = null;
  let mode: "binary" | "fields" = "binary";
  let limitGib = 16;
  let busy = false;
  let polling = false;
  let alive = true;
  let initialized = false;
  let revision = 0;
  let error = "";
  $: status = mode === "binary" ? binary : normal;
  $: active = Boolean(normal?.active || binary?.active);
  $: stopping = mode === "binary" && binary?.stopping;
  async function refresh() {
    if (busy || polling) return;
    polling = true; const version = revision;
    try {
      const [fields, raw] = await Promise.all([invoke<Recording>("memory_recording_status"), invoke<BinaryRecording>("binary_recording_status")]);
      if (alive && version === revision) {
        normal = fields; binary = raw;
        if (!initialized) { if (raw.active) mode = "binary"; else if (fields.active) mode = "fields"; initialized = true; }
      }
    } catch { if (alive && version === revision) error = "Не удалось получить состояние записи."; }
    finally { polling = false; }
  }
  async function act(action: "start" | "stop" | "sample" | "folder") {
    if (busy) return;
    busy = true; revision++; error = "";
    const command = `${mode === "binary" ? "binary" : "memory"}_recording_${action}`;
    try {
      const next = await invoke<Recording | BinaryRecording>(command, mode === "binary" && action === "start" ? {limitGib} : undefined);
      if (alive && action !== "folder") { if (mode === "binary") binary = next as BinaryRecording; else normal = next as Recording; }
    } catch (reason) { if (alive) error = typeof reason === "string" ? reason : "Не удалось выполнить действие."; }
    finally { busy = false; }
  }
  onMount(() => { void refresh(); const timer = setInterval(() => void refresh(), 2000); return () => { alive = false; revision++; clearInterval(timer); }; });
</script>

<section class="recording" aria-label="Запись исследования памяти">
  <div class="heading"><div><strong>Запись исследования миссии</strong><p>Начните в Орбитере, дождитесь снимка и отправляйтесь на миссию. После возвращения сохраните ещё один снимок.</p></div>
    <div class="options"><label>Что записывать<select bind:value={mode} disabled={active || busy}><option value="binary">Двоичные снимки памяти</option><option value="fields">Только игровые поля</option></select></label>
    {#if mode === "binary"}<label>Предел размера<select bind:value={limitGib} disabled={active || busy}><option value={8}>8 ГиБ</option><option value={16}>16 ГиБ</option><option value={32}>32 ГиБ</option></select></label>{/if}</div>
  </div>
  {#if mode === "binary"}<p class="notice">Сохраняются исходные байты памяти: они могут содержать данные входа и переписку. Файлы остаются на этом компьютере; не публикуйте их целиком. Нужны минимум 3 ГиБ свободного места. Чтение может влиять на плавность игры.</p>{/if}
  <div class="actions">
    {#if status?.active}<button class="stop" disabled={busy || stopping} onclick={() => void act("stop")}>{stopping ? "Завершаем запись…" : "Остановить и сохранить"}</button><button disabled={busy || status.scanning || stopping} onclick={() => void act("sample")}>Снять сейчас</button>
    {:else}<button disabled={busy || !status || active} onclick={() => void act("start")}>{mode === "binary" ? "Начать двоичную запись" : "Начать запись полей"}</button>{/if}
    <button disabled={busy} onclick={() => void act("folder")}>Открыть папку записи</button>
  </div>
  {#if error || status?.error}<p class="error" role="alert">{error || status?.error}<button disabled={busy} onclick={() => { error = ""; void refresh(); }}>Повторить</button></p>{/if}
  {#if status?.active}<p class="live" role="status">{stopping ? "Сохраняем прочитанную часть. Дождитесь завершения перед закрытием приложения." : status.scanning ? "Читаем память…" : "Запись идёт · ждём следующий снимок"} · Снимков: {status.samples} · {mode === "binary" ? `${(status.bytes / 1024 ** 3).toFixed(2)} ГиБ` : `${(status.bytes / 1024 ** 2).toFixed(1)} МиБ`}</p>
    {#if mode === "binary" && binary?.scanning}<p>Прочитано в текущем снимке: {(binary.readBytes / 1024 ** 3).toFixed(2)} ГиБ. Непрочитанных участков: {binary.holes}.</p>{/if}
  {:else if status?.path}<p role="status">{status.error ? "Запись завершена с ошибкой" : "Запись сохранена"} · Снимков: {status.samples}. {status.stopReason}</p>{/if}
  {#if mode === "binary" && binary?.lastComplete === false}<p class="notice">Последний снимок неполный: в индексе отмечены недоступные или не успевшие прочитаться участки. Сохранённые блоки пригодны для анализа.</p>{/if}
  <details><summary>Что сохраняется и где искать файл</summary>
    {#if mode === "binary"}<p>Первый снимок сохраняет доступные байты, адреса областей и список модулей игры. Следующие записывают изменения; одинаковые блоки хранятся один раз со сжатием. Папка записи нужна целиком для восстановления снимков.</p><p>Интервал — 45 секунд после готового снимка. Один обход ограничен 90 секундами и 24 ГиБ чтений. Запись завершается при достижении выбранного размера, остатке менее 2 ГиБ на диске, завершении игры или пределе 20 минут (с завершением текущего обхода).</p><p>При остановке сохраняется уже прочитанная часть с отметкой неполноты. Если приложение аварийно закроется, ранее завершённые снимки останутся доступны. Это последовательное чтение работающей игры: данные могут меняться прямо во время обхода.</p>
    {:else}<p>Сохраняются игровые поля, их структура, разрешённые примеры, пути ресурсов и различия между снимками. Сырые блоки памяти и учётные данные не записываются. Интервал — 30 секунд после готового снимка; чтение тоже занимает время.</p><p>Автоматическая остановка: через 60 минут, при достижении 100 МиБ или завершении игры. При остановке незавершённое чтение отбрасывается. Старые объекты могут оставаться в памяти; изменение записи ещё не доказывает игровое событие.</p>{/if}
    {#if status?.path}<code>{status.path}</code>{:else}<code>PlatScope → diagnostics → {mode === "binary" ? "binary-memory" : "memory"}</code>{/if}
  </details>
</section>

<style>
  .recording { border:1px solid var(--border); border-radius:.7rem; padding:1rem; background:var(--surface); min-width:0; font-size:.85rem; }
  .heading { display:flex; align-items:flex-start; justify-content:space-between; gap:1rem; flex-wrap:wrap; } .heading > div { min-width:0; } strong { font-size:.95rem; }
  p { margin:.5rem 0 0; line-height:1.55; color:var(--text-muted); } .actions,.options { display:flex; gap:.5rem; flex-wrap:wrap; } .actions { margin-top:.8rem; }
  label { display:flex; flex-direction:column; gap:.35rem; color:var(--text-muted); min-width:0; } select { max-width:100%; }
  button,select { font:inherit; color:var(--text); padding:.55rem .8rem; border:1px solid var(--border); border-radius:.5rem; background:var(--surface-2); } button { cursor:pointer; } button:disabled,select:disabled { opacity:.5; cursor:default; } .stop { border-color:var(--accent); }
  button:hover:not(:disabled) { background:var(--surface-2); border-color:var(--accent); }
  button:focus-visible,summary:focus-visible,select:focus-visible { outline:2px solid var(--accent); outline-offset:3px; } details { margin-top:.65rem; } summary { cursor:pointer; color:var(--text-muted); } code { display:block; overflow-wrap:anywhere; margin-top:.7rem; font-size:.75rem; } .live { color:var(--accent); } .error { color:var(--danger,#ef9e9e); overflow-wrap:anywhere; } .notice { border-left:2px solid var(--accent); padding-left:.7rem; }
</style>
