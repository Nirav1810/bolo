<script lang="ts">
  import { Check, Copy, History as HistoryIcon, Search, Trash2, Undo2, SearchX } from "@lucide/svelte";
  import { api, type Form, type HistoryItem, type SettingsView } from "../lib/api";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let query = $state("");
  let rows: HistoryItem[] = $state([]);
  let loaded = $state(false);
  let confirmClear = $state(false);
  let copied: string | null = $state(null);
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
    touched: "Polish skipped: you typed or clicked",
    "other-window": "Polish skipped: you switched windows",
    "too-late": "Polish skipped: it came too late",
    multiline: "Polish skipped: several lines",
    unchanged: "Polish made no change",
    blocked: "Polish skipped: the app was blocked",
    failed: "Polish couldn’t be swapped in",
  };

  const day = (ms: number) => new Date(ms).toDateString();
  function dayLabel(ms: number): string {
    const d = new Date(ms);
    if (d.toDateString() === new Date().toDateString()) return "Today";
    if (d.toDateString() === new Date(Date.now() - 86_400_000).toDateString()) return "Yesterday";
    return d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
  }
  const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const appName = (exe: string) => exe.replace(/\.exe$/, "");

  /** Rows grouped by day, newest first, as the search returns them. */
  const groups = $derived.by(() => {
    const out: { label: string; rows: HistoryItem[] }[] = [];
    for (const r of rows) {
      const last = out[out.length - 1];
      if (last && day(last.rows[0].at_ms) === day(r.at_ms)) last.rows.push(r);
      else out.push({ label: dayLabel(r.at_ms), rows: [r] });
    }
    return out;
  });

  async function copy(key: string, text: string) {
    await api.copy(text);
    copied = key;
    setTimeout(() => copied === key && (copied = null), 1500);
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

<div class="flex flex-col gap-3">
  <div class="flex items-center gap-2">
    <div class="relative flex-1">
      <Search class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
      <label for="history-search" class="sr-only">Search history</label>
      <input id="history-search" class="input pl-8!" type="search" placeholder="Search dictations…" autocomplete="off" spellcheck="false" bind:value={query} />
    </div>
    {#if confirmClear}
      <span class="text-xs text-muted-foreground">Delete every dictation?</span>
      <Button variant="ghost-muted" onclick={() => (confirmClear = false)}>Cancel</Button>
      <Button variant="destructive-outline" onclick={clearAll}>Delete all</Button>
    {:else}
      <Button variant="destructive-outline" disabled={!rows.length && !query} onclick={() => (confirmClear = true)}>
        <Trash2 aria-hidden="true" />Clear…
      </Button>
    {/if}
  </div>
  {#if error}<Alert variant="error">{error}</Alert>{/if}

  {#if !loaded}
    <div class="flex flex-col gap-2" aria-hidden="true">
      {#each [0, 1, 2] as i (i)}<div class="h-16 animate-pulse rounded-xl bg-muted"></div>{/each}
    </div>
  {:else if !rows.length}
    <div class="rounded-xl border border-dashed border-border">
      {#if query}
        <Empty icon={SearchX} title="Nothing matches" hint="Try fewer or different words." />
      {:else}
        <Empty
          icon={HistoryIcon}
          title={view.form.history ? "No dictations yet" : "History is off"}
          hint={view.form.history ? "Your dictations show up here, newest first." : "Turn on “Save dictation history” below to keep them."}
        />
      {/if}
    </div>
  {:else}
    {#each groups as g (g.label)}
      <section class="flex flex-col gap-1.5">
        <h2 class="px-1 pt-1 text-xs font-medium text-muted-foreground">{g.label}</h2>
        <ul class="overflow-hidden rounded-xl border border-border/80 bg-card shadow-xs/5 dark:border-border [&>*+*]:border-t [&>*+*]:border-border/60">
          {#each g.rows as r (r.id)}
            {@const text = r.polished_text ?? r.final_text}
            {@const edited = !!r.polished_text && r.polished_text !== r.final_text}
            <li class="group/item relative flex gap-3 px-4 py-3 transition-colors hover:bg-muted/60 dark:hover:bg-white/[0.02]">
              <span class="w-15 shrink-0 pt-0.5 text-2xs whitespace-nowrap text-muted-foreground tabular-nums">{time(r.at_ms)}</span>
              <div class="flex min-w-0 flex-1 flex-col gap-1.5">
                <p
                  class="text-sm leading-[21px] wrap-break-word whitespace-pre-wrap text-foreground"
                  class:mono={r.app === "windowsterminal.exe" || r.app === "code.exe"}
                >
                  {text}
                </p>
                {#if edited}
                  <p class="text-xs wrap-break-word whitespace-pre-wrap text-muted-foreground line-through decoration-muted-foreground/40">
                    {r.final_text}
                  </p>
                {/if}
                <div class="flex flex-wrap items-center gap-1.5">
                  <Badge variant="outline">{appName(r.app)}</Badge>
                  <Badge variant="secondary">{r.language[0].toUpperCase() + r.language.slice(1)}</Badge>
                  {#if r.swap}
                    <Badge variant={r.swap === "swapped" ? "primary" : "secondary"}>{SWAP[r.swap] ?? `Polish: ${r.swap}`}</Badge>
                  {/if}
                </div>
              </div>
              <div
                class="flex shrink-0 items-start gap-0.5 opacity-0 transition-opacity duration-100 group-hover/item:opacity-100 focus-within:opacity-100"
              >
                <Button variant="ghost-muted" size="icon-sm" aria-label={copied === `${r.id}` ? "Copied" : "Copy"} title="Copy" onclick={() => copy(`${r.id}`, text)}>
                  {#if copied === `${r.id}`}<Check class="text-success-foreground" aria-hidden="true" />{:else}<Copy aria-hidden="true" />{/if}
                </Button>
                {#if edited}
                  <Button
                    variant="ghost-muted"
                    size="icon-sm"
                    aria-label={copied === `${r.id}-o` ? "Copied" : "Copy the text before polish"}
                    title="Copy the text before polish"
                    onclick={() => copy(`${r.id}-o`, r.final_text)}
                  >
                    {#if copied === `${r.id}-o`}<Check class="text-success-foreground" aria-hidden="true" />{:else}<Undo2 aria-hidden="true" />{/if}
                  </Button>
                {/if}
                <ConfirmButton label="Delete this dictation" onconfirm={() => remove(r.id)}><Trash2 aria-hidden="true" /></ConfirmButton>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</div>

<SettingsSection title="Keep">
  <SettingsRow title="Save dictation history" labelFor="history-on" description="Text only, on this PC. Turning it off keeps what’s saved.">
    {#snippet control()}
      <Switch id="history-on" checked={view.form.history} onchange={(on) => update((f) => (f.history = on))} />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Delete after" labelFor="retention" description="Older dictations are removed automatically.">
    {#snippet control()}
      <select
        id="retention"
        class="select"
        value={view.form.retention_days}
        onchange={(e) => update((f) => (f.retention_days = Number(e.currentTarget.value)))}
      >
        {#each RETENTION as d (d)}
          <option value={d}>{d === 1 ? "1 day" : d === 365 ? "1 year" : `${d} days`}</option>
        {/each}
        {#if !RETENTION.includes(view.form.retention_days)}
          <option value={view.form.retention_days}>{view.form.retention_days} days</option>
        {/if}
      </select>
    {/snippet}
  </SettingsRow>
</SettingsSection>
