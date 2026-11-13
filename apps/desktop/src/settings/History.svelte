<script lang="ts">
  import { tick } from "svelte";
  import { Check, Copy, History as HistoryIcon, Search, SearchX, Trash2, ClipboardType } from "@lucide/svelte";
  import { LANGUAGE_NAMES, api, engineName, plural, sentence, type Form, type HistoryItem, type SettingsView } from "../lib/api";
  import Alert from "../lib/ui/Alert.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import ConfirmDialog from "../lib/ui/ConfirmDialog.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import Select from "../lib/ui/Select.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Skeleton from "../lib/ui/Skeleton.svelte";
  import Switch from "../lib/ui/Switch.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let query = $state("");
  let rows: HistoryItem[] = $state([]);
  let loaded = $state(false);
  let confirmAll = $state(false);
  let copied: string | null = $state(null);
  let error = $state<{ title: string; detail: string } | null>(null);
  let searchInput: HTMLInputElement | undefined = $state();

  const RETENTION = [1, 7, 30, 90, 365];
  const retentionName = (d: number) => (d === 365 ? "1 year" : plural(d, "day", "days"));
  const retentionOptions = $derived<[string, string][]>([
    ...RETENTION.map((d): [string, string] => [String(d), retentionName(d)]),
    ...(RETENTION.includes(view.form.retention_days) ? [] : [[String(view.form.retention_days), retentionName(view.form.retention_days)] as [string, string]]),
  ]);

  async function search() {
    try {
      rows = await api.history(query);
      if (error?.title === "Couldn’t load history") error = null;
    } catch (e) {
      error = { title: "Couldn’t load history", detail: sentence(String(e)) };
    }
    loaded = true;
  }

  // Search as the user types, a beat after the last key.
  $effect(() => {
    void query;
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
    failed: "Polished text couldn’t be swapped in",
  };

  const day = (ms: number) => new Date(ms).toDateString();
  function dayLabel(ms: number): string {
    const d = new Date(ms);
    if (d.toDateString() === new Date().toDateString()) return "Today";
    if (d.toDateString() === new Date(Date.now() - 86_400_000).toDateString()) return "Yesterday";
    return d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
  }
  const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  /** The app as the Apps page names it. */
  const appOf = (exe: string) => view.apps.find((a) => a.exes.includes(exe));

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

  async function copy(key: string, text: string, what: string) {
    try {
      await api.copy(text);
    } catch (e) {
      error = { title: `Couldn’t copy ${what}`, detail: sentence(String(e)) };
      return;
    }
    copied = key;
    setTimeout(() => copied === key && (copied = null), 1500);
  }

  async function remove(id: number) {
    const at = rows.findIndex((r) => r.id === id);
    try {
      await api.deleteRow(id);
    } catch (e) {
      error = { title: "Couldn’t delete this dictation", detail: sentence(String(e)) };
      return;
    }
    rows = rows.filter((r) => r.id !== id);
    // The row under the keyboard's focus is gone: go to the next one, else the one
    // before, else the search field.
    await tick();
    const next = rows[Math.min(at, rows.length - 1)];
    (next ? document.querySelector<HTMLElement>(`[data-dictation-copy="${next.id}"]`) : searchInput)?.focus();
  }

  async function deleteAll() {
    try {
      await api.clearHistory();
    } catch (e) {
      error = { title: "Couldn’t delete history", detail: sentence(String(e)) };
      return;
    }
    rows = [];
    // "Delete all…" is disabled now, so the dialog can't hand focus back to it.
    await tick();
    searchInput?.focus();
  }
</script>

{#if error}
  <Alert variant="error" title={error.title}>{error.detail}</Alert>
{/if}

<div class="flex items-center gap-2">
  <div class="relative flex-1">
    <div class="pointer-events-none absolute inset-y-0 left-3 grid place-items-center text-muted-foreground">
      <Search class="size-4" aria-hidden="true" />
    </div>
    <label for="history-search" class="sr-only">Search dictations</label>
    <input
      bind:this={searchInput}
      id="history-search"
      class="field pl-9"
      type="search"
      placeholder="Search dictations…"
      autocomplete="off"
      spellcheck="false"
      bind:value={query}
    />
  </div>
  <Button variant="outline" disabled={!rows.length && !query} onclick={() => (confirmAll = true)}>
    <Trash2 aria-hidden="true" />Delete all…
  </Button>
</div>

<ConfirmDialog bind:open={confirmAll} title="Delete all dictations?" confirm="Delete all" onconfirm={deleteAll}>
  Every saved dictation is deleted from this PC, and this can’t be undone.
</ConfirmDialog>

{#if !loaded}
  <Skeleton />
{:else if !rows.length}
  <Card>
    {#if query}
      <Empty icon={SearchX} title="No dictations match" hint="Try fewer or different words." />
    {:else}
      <Empty
        icon={HistoryIcon}
        title={view.form.history ? "No dictations yet" : "History is off"}
        hint={view.form.history ? "Your dictations show up here, newest first." : "Turn on “Save dictation history” under Retention to keep them."}
      />
    {/if}
  </Card>
{:else}
  {#each groups as g (g.label)}
    <SettingsSection title={g.label} as="ul">
      {#each g.rows as r (r.id)}
        {@const text = r.polished_text ?? r.final_text}
        {@const edited = !!r.polished_text && r.polished_text !== r.final_text}
        {@const app = appOf(r.app)}
        <SettingsRow as="li" align="start">
          {#snippet title()}
            <div class="flex flex-col gap-1 font-normal">
              <p class="wrap-break-word whitespace-pre-wrap {app?.code_mode ? 'font-mono' : ''}">{text}</p>
              {#if edited}
                <p class="text-xs wrap-break-word whitespace-pre-wrap text-muted-foreground line-through">
                  <span class="sr-only">Instant text: </span>{r.final_text}
                </p>
              {/if}
            </div>
          {/snippet}
          {#snippet description()}
            <span class="tabular-nums">{time(r.at_ms)}</span>
            · {#if app}{app.name.value}{:else}<span class="font-mono">{r.app}</span>{/if}
            · {LANGUAGE_NAMES[r.language] ?? r.language}
            · {engineName(r.engine, view)}{#if r.swap}{" "}· {SWAP[r.swap] ?? `Polish: ${r.swap}`}{/if}
          {/snippet}
          {#snippet control()}
            {#if edited}
              <Button
                variant="quiet"
                size="icon-sm"
                aria-label={copied === `${r.id}-o` ? "Copied" : "Copy the instant text"}
                onclick={() => copy(`${r.id}-o`, r.final_text, "the instant text")}
              >
                {#if copied === `${r.id}-o`}<Check aria-hidden="true" />{:else}<ClipboardType aria-hidden="true" />{/if}
              </Button>
            {/if}
            <Button
              variant="quiet"
              size="icon-sm"
              data-dictation-copy={r.id}
              aria-label={copied === `${r.id}` ? "Copied" : "Copy this dictation"}
              onclick={() => copy(`${r.id}`, text, "this dictation")}
            >
              {#if copied === `${r.id}`}<Check aria-hidden="true" />{:else}<Copy aria-hidden="true" />{/if}
            </Button>
            <ConfirmButton label="Delete this dictation" onconfirm={() => remove(r.id)} />
          {/snippet}
        </SettingsRow>
      {/each}
    </SettingsSection>
  {/each}
{/if}

<SettingsSection title="Retention">
  <SettingsRow title="Save dictation history" labelFor="history-on" description="Text only, on this PC. Turning it off keeps what’s saved.">
    {#snippet control()}
      <Switch id="history-on" checked={view.form.history} onchange={(on) => update((f) => (f.history = on))} />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Delete after" labelFor="retention" description="Older dictations are deleted automatically.">
    {#snippet control()}
      <Select
        id="retention"
        class="w-32"
        value={String(view.form.retention_days)}
        options={retentionOptions}
        onchange={(v) => update((f) => (f.retention_days = Number(v)))}
      />
    {/snippet}
  </SettingsRow>
</SettingsSection>
