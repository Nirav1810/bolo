<script lang="ts">
  import { api, type Form, type HistoryItem, type SettingsView } from "../lib/api";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let query = $state("");
  let rows: HistoryItem[] = $state([]);
  let loaded = $state(false);
  let confirmClear = $state(false);
  let copied: number | null = $state(null);
  let error = $state("");

  const RETENTION = [1, 7, 30, 90, 365];

  async function search() {
    try {
      rows = await api.history(query);
      error = "";
    } catch (e) {
      error = String(e);
    }
    loaded = true;
  }

  // Search as the user types, a beat after the last key.
  $effect(() => {
    query;
    const t = setTimeout(search, loaded ? 200 : 0);
    return () => clearTimeout(t);
  });

  const SWAP: Record<string, string> = {
    swapped: "Polished",
    touched: "Polish skipped, you typed or clicked",
    "other-window": "Polish skipped, you switched windows",
    "too-late": "Polish skipped, it came too late",
    multiline: "Polish skipped, several lines",
    unchanged: "Polish made no change",
    blocked: "Polish skipped, the app was blocked",
    failed: "Polish couldn't be swapped in",
  };

  function when(ms: number): string {
    const d = new Date(ms);
    const today = new Date();
    const yesterday = new Date(Date.now() - 86_400_000);
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (d.toDateString() === today.toDateString()) return `Today ${time}`;
    if (d.toDateString() === yesterday.toDateString()) return `Yesterday ${time}`;
    return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
  }

  function caption(r: HistoryItem): string {
    const parts = [when(r.at_ms), r.app.replace(/\.exe$/, ""), r.language[0].toUpperCase() + r.language.slice(1)];
    if (r.swap) parts.push(SWAP[r.swap] ?? `Polish: ${r.swap}`);
    return parts.join(" · ");
  }

  async function copy(r: HistoryItem, text: string) {
    await api.copy(text);
    copied = r.id;
    setTimeout(() => copied === r.id && (copied = null), 1500);
  }

  async function remove(id: number) {
    await api.deleteRow(id);
    rows = rows.filter((r) => r.id !== id);
  }

  async function clearAll() {
    await api.clearHistory();
    confirmClear = false;
    rows = [];
  }
</script>

<div class="flex items-center gap-2">
  <label for="history-search" class="sr-only">Search history</label>
  <input id="history-search" class="input flex-1" type="search" placeholder="Search dictations" bind:value={query} />
  {#if confirmClear}
    <span class="caption">Delete every dictation?</span>
    <button type="button" class="btn danger" onclick={clearAll}>Delete all</button>
    <button type="button" class="btn" onclick={() => (confirmClear = false)}>Cancel</button>
  {:else}
    <button type="button" class="btn danger" disabled={!rows.length && !query} onclick={() => (confirmClear = true)}>
      Clear history…
    </button>
  {/if}
</div>
{#if error}<p class="note danger mt-2" role="alert"><span class="icon" aria-hidden="true">⚠</span>{error}</p>{/if}

<div class="mt-3 overflow-hidden rounded-[4px] border border-border bg-card">
  {#each rows as r (r.id)}
    <div class="flex items-start gap-3 border-b border-border px-4 py-3 last:border-b-0">
      <div class="card-text">
        <span class="whitespace-pre-wrap" class:mono={r.app === "windowsterminal.exe" || r.app === "code.exe"}>
          {r.polished_text ?? r.final_text}
        </span>
        {#if r.polished_text && r.polished_text !== r.final_text}
          <span class="caption whitespace-pre-wrap">Before polish: {r.final_text}</span>
        {/if}
        <span class="caption">{caption(r)}</span>
      </div>
      <button type="button" class="btn" onclick={() => copy(r, r.polished_text ?? r.final_text)}>
        {copied === r.id ? "Copied" : "Copy"}
      </button>
      {#if r.polished_text && r.polished_text !== r.final_text}
        <button type="button" class="btn" onclick={() => copy(r, r.final_text)}>Copy original</button>
      {/if}
      <button type="button" class="btn" aria-label="Delete this dictation" onclick={() => remove(r.id)}>✕</button>
    </div>
  {:else}
    <p class="caption px-4 py-6 text-center">
      {!loaded ? "Loading…" : query ? "Nothing matches." : view.form.history ? "Your dictations will show up here." : "History is off."}
    </p>
  {/each}
</div>

<h2 class="section">Keep</h2>
<div class="group">
  <div class="card">
    <div class="card-text">
      <label for="history-on">Save dictation history</label>
      <span class="caption">Text only, on this PC. Turning it off keeps what’s saved.</span>
    </div>
    <input
      id="history-on"
      class="switch"
      type="checkbox"
      checked={view.form.history}
      onchange={(e) => update((f) => (f.history = e.currentTarget.checked))}
    />
  </div>
  <div class="card">
    <div class="card-text"><label for="retention">Delete after</label></div>
    <select
      id="retention"
      class="select"
      value={view.form.retention_days}
      onchange={(e) => update((f) => (f.retention_days = Number(e.currentTarget.value)))}
    >
      {#each RETENTION as d}
        <option value={d}>{d === 1 ? "1 day" : d === 365 ? "1 year" : `${d} days`}</option>
      {/each}
      {#if !RETENTION.includes(view.form.retention_days)}
        <option value={view.form.retention_days}>{view.form.retention_days} days</option>
      {/if}
    </select>
  </div>
</div>
