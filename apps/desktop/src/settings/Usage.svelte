<script lang="ts">
  // The usage meter: what each paid cloud request was billed for, as the provider
  // reported it (audio length, Claude token counts), recorded per request by Rust
  // (src-tauri/src/journal.rs) and priced at list price. An optional admin key adds
  // the provider's own account totals (src-tauri/src/billing.rs).
  import { ExternalLink, Gauge, RefreshCw, RotateCcw } from "@lucide/svelte";
  import { tick } from "svelte";
  import {
    PROVIDER_NAMES,
    api,
    plural,
    saveCheckedKey,
    sentence,
    type ProviderReports,
    type ProviderUsage,
    type UsageView,
  } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  import { PAGES } from "../lib/pages";
  import Alert from "../lib/ui/Alert.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import ConfirmDialog from "../lib/ui/ConfirmDialog.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import Inset from "../lib/ui/Inset.svelte";
  import KeyField from "../lib/ui/KeyField.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Skeleton from "../lib/ui/Skeleton.svelte";

  let { onkeys }: { onkeys: () => void } = $props();

  type Period = "today" | "last_30_days" | "all_time";
  type RowStatus = { tone: "success" | "warning" | "error" | "busy"; text: string };

  let period = $state<Period>("last_30_days");
  let usage = $state<UsageView | null>(null);
  let failure = $state<{ title: string; detail: string } | null>(null);
  let confirmReset = $state(false);
  let reports = $state<ProviderReports | null>(null);
  let refreshing = $state(false);
  /** Admin-key panels the user opened, by provider. */
  let adding: Record<string, boolean> = $state({});
  /** How the last admin-key action went, by provider. */
  let adminStatus: Record<string, RowStatus | null> = $state({});
  /** Provider → hint of the API key stored on Engines and keys (null: none). */
  let storedKeys: Record<string, string | null> = $state({});

  async function loadKeys() {
    try {
      storedKeys = (await api.settings()).keys;
    } catch {
      // Only used to decide which provider cards to show.
    }
  }

  async function loadReports(refresh: boolean) {
    refreshing = true;
    try {
      reports = await api.usageReports(refresh);
    } catch {
      // The meter still works; the reports are an extra.
    }
    refreshing = false;
  }

  async function load() {
    const midnight = new Date();
    midnight.setHours(0, 0, 0, 0);
    try {
      usage = await api.usage(midnight.getTime());
      if (failure?.title === "Couldn’t load usage") failure = null;
    } catch (e) {
      failure = { title: "Couldn’t load usage", detail: sentence(String(e)) };
    }
  }

  $effect(() => {
    load();
    loadKeys();
    loadReports(false);
    // Each paid request lands as a `usage` event: keep the numbers live.
    const off = onAppEvent((e) => e.kind === "usage" && load());
    const onFocus = () => {
      load();
      loadKeys();
    };
    addEventListener("focus", onFocus);
    return () => {
      removeEventListener("focus", onFocus);
      off.then((f) => f());
    };
  });

  async function reset() {
    try {
      await api.resetUsage();
      failure = null;
    } catch (e) {
      failure = { title: "Couldn’t reset usage", detail: sentence(String(e)) };
    }
    await load();
  }

  /** The admin key is saved under its own id, separate from the engine's API key. */
  const adminId = (provider: string) => `${provider}-admin`;

  async function openAdmin(provider: string) {
    adding[provider] = !adding[provider];
    adminStatus[provider] = null;
    await tick();
    // Opening puts the cursor in the field; closing hands focus back to the opener.
    document.getElementById(adding[provider] ? `admin-key-${provider}` : `admin-key-open-${provider}`)?.focus();
  }

  async function closeAdmin(provider: string) {
    adding[provider] = false;
    await tick();
    document.getElementById(`admin-key-open-${provider}`)?.focus();
  }

  /** Checks and saves a pasted admin key; resolves to what went wrong, for the KeyField. */
  async function saveAdminKey(provider: string, key: string): Promise<string | null> {
    const r = await saveCheckedKey(adminId(provider), key, PROVIDER_NAMES[provider]);
    if (!r.saved) return r.error;
    adding[provider] = false;
    adminStatus[provider] = r.note ? { tone: "warning", text: r.note } : null;
    await loadReports(true);
    return null;
  }

  async function deleteAdminKey(provider: string) {
    adminStatus[provider] = null;
    try {
      await api.saveKey(adminId(provider), "");
    } catch (e) {
      adminStatus[provider] = { tone: "error", text: `Couldn’t delete the admin key: ${sentence(String(e))}` };
    }
    await loadReports(true);
  }

  interface ProviderInfo {
    console: string;
    speech: boolean;
  }
  const PROVIDERS: Record<string, ProviderInfo> = {
    deepgram: { console: "https://console.deepgram.com/", speech: true },
    anthropic: { console: "https://platform.claude.com/usage", speech: false },
    groq: { console: "https://console.groq.com/dashboard/usage", speech: true },
    sarvam: { console: "https://dashboard.sarvam.ai/", speech: true },
  };
  const info = (id: string): ProviderInfo => PROVIDERS[id] ?? { console: "", speech: true };

  /** The providers whose own account totals an admin key can show, and what it adds. */
  const ADMIN_HINTS: Record<string, string> = {
    deepgram: "A Deepgram admin key shows the account’s own totals and credit left.",
    anthropic: "An Anthropic admin key (sk-ant-admin…) shows the account’s own totals.",
  };

  /** Bolo's cloud providers, shown once they have a key or have been used; anything else once used. */
  const CLOUD = ["deepgram", "groq", "sarvam", "anthropic"];
  const rows = $derived.by(() => {
    const got = usage?.[period] ?? [];
    const used = (p: string) => usage?.all_time.some((u) => u.provider === p) ?? false;
    const out: ProviderUsage[] = CLOUD.filter((p) => storedKeys[p] || used(p)).map(
      (p) =>
        got.find((u) => u.provider === p) ?? {
          provider: p,
          requests: 0,
          audio_secs: 0,
          input_tokens: 0,
          output_tokens: 0,
          cache_read_tokens: 0,
          cache_write_tokens: 0,
          cost_usd: 0,
        },
    );
    return out.concat(got.filter((u) => !CLOUD.includes(u.provider)));
  });
  const total = $derived(rows.reduce((sum, r) => sum + r.cost_usd, 0));
  const empty = $derived(!!usage && usage.all_time.length === 0);
  const hours = (h: number) => duration(h * 3600);

  function money(usd: number): string {
    if (usd === 0) return "$0.00";
    if (usd < 0.0001) return "<$0.0001";
    if (usd < 0.01) return `$${usd.toFixed(4)}`;
    return `$${usd.toFixed(2)}`;
  }

  function duration(secs: number): string {
    if (secs < 60) return `${Math.round(secs)} s`;
    const min = secs / 60;
    if (min < 60) return `${min.toFixed(min < 10 ? 1 : 0)} min`;
    const h = Math.floor(min / 60);
    return `${h} h ${Math.round(min - h * 60)} min`;
  }

  function count(n: number): string {
    if (n < 1000) return String(n);
    if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}K`;
    return `${(n / 1_000_000).toFixed(1)}M`;
  }

  const report = (provider: string) => (provider === "deepgram" ? reports?.deepgram : reports?.anthropic);
  const source = (provider: string) => (provider === "deepgram" ? reports?.deepgram_key : reports?.anthropic_key);

  /** The account's own totals for the period, as one line of facts. */
  function accountLine(provider: string): string {
    if (provider === "deepgram" && reports?.deepgram.state === "ok") {
      const d = reports.deepgram.report;
      const f = d.periods[period];
      const facts = [hours(f.hours), plural(Math.round(f.requests), "request", "requests")];
      if (f.usd !== null) facts.push(money(f.usd));
      if (d.balance !== null) facts.push(`${money(d.balance)} credit left`);
      return facts.join(" · ");
    }
    if (provider === "anthropic" && reports?.anthropic.state === "ok") {
      const a = reports.anthropic.report;
      const f = a.periods[period];
      return `${count(f.input_tokens + f.cache_read_tokens + f.cache_write_tokens)} in · ${count(f.output_tokens)} out · ${money(a.account_usd[period])}`;
    }
    return "";
  }

  /** The provider's days are UTC days, and Anthropic reports a year at most. */
  const accountTitle = (provider: string) =>
    period === "today"
      ? "Account totals, today in UTC"
      : period === "all_time" && provider === "anthropic"
        ? "Account totals, last 12 months"
        : "Account totals";

  const since = $derived(
    usage?.since_ms ? new Date(usage.since_ms).toLocaleDateString([], { day: "numeric", month: "long", year: "numeric" }) : null,
  );
</script>

<div class="flex items-center justify-between gap-3">
  <Segmented
    label="Period"
    size="default"
    value={period}
    options={[
      ["today", "Today"],
      ["last_30_days", "Last 30 days"],
      ["all_time", "All time"],
    ]}
    onchange={(v) => (period = v as Period)}
  />
  {#if usage && !empty}
    <div class="flex flex-col items-end gap-0.5">
      <span class="text-xs text-muted-foreground">Estimated total</span>
      <span class="text-heading text-foreground tabular-nums">{money(total)}</span>
    </div>
  {/if}
</div>

{#if failure}<Alert variant="error" title={failure.title}>{failure.detail}</Alert>{/if}

{#if !usage}
  {#if !failure}<Skeleton rows={2} tall />{/if}
{:else if rows.length === 0}
  <Card>
    <Empty icon={Gauge} title="No cloud usage yet" hint="Add an API key on {PAGES.engines.label} to use a cloud engine.">
      <Button variant="outline" size="sm" onclick={onkeys}>Open {PAGES.engines.label}</Button>
    </Empty>
  </Card>
{:else}
  {#each rows as r (r.provider)}
    {@const p = info(r.provider)}
    {@const name = PROVIDER_NAMES[r.provider] ?? r.provider}
    <SettingsSection title={name}>
      {#snippet action()}
        {#if p.console}
          <Button variant="ghost" size="sm" onclick={() => api.openLink(p.console)}>Dashboard<ExternalLink aria-hidden="true" /></Button>
        {/if}
      {/snippet}
      <div class="grid grid-cols-3 divide-x divide-divider">
        {#if p.speech}
          {@render stat("Audio", duration(r.audio_secs), plural(r.requests, "request", "requests"))}
          {@render stat("Average", r.requests ? duration(r.audio_secs / r.requests) : "–", r.requests ? "per request" : "")}
        {:else}
          {@render stat("Requests", count(r.requests), "")}
          {@render stat(
            "Tokens",
            count(r.input_tokens + r.cache_read_tokens + r.cache_write_tokens + r.output_tokens),
            r.requests ? `${count(r.input_tokens + r.cache_read_tokens + r.cache_write_tokens)} in · ${count(r.output_tokens)} out` : "",
          )}
        {/if}
        {@render stat("Estimated cost", money(r.cost_usd), r.requests ? `${money(r.cost_usd / r.requests)} each` : "")}
      </div>
      {#if ADMIN_HINTS[r.provider]}{@render account(r.provider, name)}{/if}
    </SettingsSection>
  {/each}
{/if}

{#if usage}
  <div class="flex items-center justify-between gap-4 px-1">
    <p class="text-xs text-muted-foreground">Estimated at list prices{since ? `, since ${since}` : ""}.</p>
    {#if !empty}
      <Button variant="outline" onclick={() => (confirmReset = true)}><RotateCcw aria-hidden="true" />Reset usage…</Button>
    {/if}
  </div>
{/if}

<ConfirmDialog bind:open={confirmReset} title="Reset all usage totals?" confirm="Reset usage" onconfirm={reset}>
  Every provider’s totals go back to zero; the providers’ own account totals stay.
</ConfirmDialog>

{#snippet stat(label: string, value: string, sub: string)}
  <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
    <span class="text-xs text-muted-foreground">{label}</span>
    <span class="text-heading text-foreground tabular-nums">{value}</span>
    <span class="truncate text-xs text-muted-foreground tabular-nums">{sub || " "}</span>
  </div>
{/snippet}

<!-- The provider's own account totals, read with the engine's API key or a separate admin key. -->
{#snippet account(provider: string, name: string)}
  {@const rep = report(provider)}
  {@const ok = rep?.state === "ok"}
  {@const ownKey = source(provider) === "usage"}
  {@const refused = ownKey && (rep?.state === "refused" || rep?.state === "failed") ? sentence(rep.message) : ""}
  {@const status = adminStatus[provider] ?? (refused ? { tone: "error" as const, text: refused } : null)}
  <SettingsRow title={ok ? accountTitle(provider) : "Account totals"} {status}>
    {#snippet description()}
      {#if ok}<span class="tabular-nums">{accountLine(provider)}</span>{:else}{ADMIN_HINTS[provider]}{/if}
    {/snippet}
    {#snippet control()}
      {#if ok}
        <Button variant="quiet" size="icon-sm" aria-label="Refresh account totals" loading={refreshing} onclick={() => loadReports(true)}>
          <RefreshCw aria-hidden="true" />
        </Button>
      {/if}
      {#if ownKey}
        <ConfirmButton label="Delete the {name} admin key" text="Delete admin key" onconfirm={() => deleteAdminKey(provider)} />
      {:else if !ok}
        <Button
          id="admin-key-open-{provider}"
          variant="ghost"
          size="sm"
          aria-expanded={!!adding[provider]}
          aria-controls="admin-key-panel-{provider}"
          onclick={() => openAdmin(provider)}>Add admin key…</Button
        >
      {/if}
    {/snippet}
  </SettingsRow>
  <!-- A saved admin key the provider turned down can be replaced right here. -->
  {#if (adding[provider] && !ownKey) || (ownKey && !ok)}
    <Inset id="admin-key-panel-{provider}">
      <KeyField
        id="admin-key-{provider}"
        label="{name} admin key"
        placeholder="Paste {name} admin key…"
        submit="Save"
        onsave={(key) => saveAdminKey(provider, key)}
        oncancel={ownKey ? undefined : () => closeAdmin(provider)}
      />
    </Inset>
  {/if}
{/snippet}
