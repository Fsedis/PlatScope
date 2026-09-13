<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  export let disabled = false;
  export let verified = false;
  type Status = "online" | "ingame" | "invisible";
  interface Presence { connection: "connecting" | "connected" | "reconnecting"; status: Status | null; statusUntil: string | null; error: string | null }
  const labels: Record<Status, string> = { online: "В сети", ingame: "В игре", invisible: "Не в сети" };
  let presence: Presence | null = null;
  let saving = false;
  let reading = false;
  let error = "";
  let readFailed = false;
  let message = "";
  let revision = 0;
  let disposed = false;
  $: connected = presence?.connection === "connected";
  $: hint = !verified ? "Для изменения статуса подтвердите аккаунт на Warframe Market."
    : saving ? "Меняем статус…"
    : presence?.connection === "reconnecting" ? presence.error ?? "Восстанавливаем связь с Warframe Market…"
    : !connected ? "Получаем статус с Warframe Market…"
    : presence?.statusUntil ? `Статус действует до ${new Date(presence.statusUntil).toLocaleTimeString("ru-RU", {hour:"2-digit",minute:"2-digit"})}.`
    : "Статус поддерживается, пока PlatScope открыт.";

  async function refresh(): Promise<void> {
    if (reading || saving || disabled) return;
    reading = true;
    const request = revision;
    try {
      const result = await invoke<Presence>("account_presence");
      if (!disposed && request === revision) {
        if (presence?.status !== result.status) message = "";
        presence = result;
        if (readFailed) { error = ""; readFailed = false; }
      }
    } catch {
      if (!disposed && request === revision) {
        presence = null;
        readFailed = true;
        error = "Не удалось получить статус. Проверьте подключение аккаунта.";
      }
    } finally { reading = false; }
  }

  async function change(event: Event): Promise<void> {
    const select = event.currentTarget as HTMLSelectElement;
    const status = select.value as Status;
    // До ответа сервера оставляем подтверждённое значение, включая ошибки записи.
    select.value = presence?.status ?? "";
    if (!connected || !verified || saving || disabled || !(status in labels)) return;
    ++revision;
    saving = true;
    error = "";
    readFailed = false;
    message = "";
    try {
      const result = await invoke<Presence>("account_set_presence", { status });
      if (!disposed) { presence = result; message = result.status ? `Статус на Warframe Market: ${labels[result.status]}.` : ""; }
    } catch {
      if (!disposed) error = "Warframe Market не подтвердил изменение. Проверьте текущий статус и повторите попытку.";
    } finally {
      saving = false;
      if (!disposed) await refresh();
    }
  }

  onMount(() => {
    void refresh();
    const timer = window.setInterval(() => { if (!document.hidden) void refresh(); }, 3000);
    return () => { disposed = true; ++revision; window.clearInterval(timer); };
  });
</script>

<div class="market-presence" aria-busy={saving}>
  <label><span>Статус на маркете</span>
    <select aria-label="Статус на маркете" value={connected ? presence?.status ?? "" : ""} disabled={disabled || saving || !connected || !verified} on:change={change}>
      {#if !connected || !presence?.status}<option value="" disabled>{error ? "Статус недоступен" : "Проверяем статус…"}</option>{/if}
      <option value="ingame">В игре</option><option value="online">В сети</option><option value="invisible">Не в сети</option>
    </select>
  </label>
  <p class:error={Boolean(error)} role={error ? "alert" : "status"}>{error || hint}</p>
  {#if error}<button class="secondary" disabled={saving || reading || disabled} on:click={() => { error = ""; void refresh(); }}>Проверить статус</button>{/if}
  <span class="sr-only" role="status">{message}</span>
</div>

<style>
  .market-presence { min-width:0; max-width:19rem; }
  label { display:flex; align-items:center; gap:.5rem; flex-wrap:wrap; font-size:.75rem; color:var(--text-muted); }
  select { min-height:34px; max-width:100%; padding:.35rem .6rem; border:1px solid var(--border); border-radius:.45rem; background:var(--surface-1); color:var(--text); font:inherit; font-weight:700; cursor:pointer; }
  select:disabled { cursor:default; opacity:.65; }
  select:focus-visible { outline:2px solid var(--accent); outline-offset:2px; }
  p { margin:.25rem 0 0; font-size:.69rem; line-height:1.4; color:var(--text-muted); overflow-wrap:anywhere; }
  p.error { color:var(--danger); }
  button { margin-top:.3rem; font-size:.75rem; }
</style>
