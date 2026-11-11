<script lang="ts">
  // The usage meter: what each paid cloud request was billed for, as the provider
  // reported it in its answer (Deepgram's audio duration, Claude's token counts),
  // recorded per request by Rust (src-tauri/src/journal.rs). Works with any key.
  // With an optional usage key, the provider's own report shows too
  // (src-tauri/src/billing.rs): the whole account, and Deepgram's credit left.
  import { AudioLines, ExternalLink, Gauge, KeyRound, RefreshCw, RotateCcw, Sparkles } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { api, type ProviderReports, type ProviderUsage, type UsageView } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  import Alert from "../lib/ui/Alert.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";

  type Period = "today" | "last_30_days" | "all_time";
  let period = $state<Period>("last_30_days");
  let usage = $state<UsageView | null>(null);
  let error = $state("");
  let confirmReset = $state(false);
  let reports = $state<ProviderReports | null>(null);
  let refreshing = $state(false);
  let keyInput: Record<string, string> = $state({});
  let keyNote: Record<string, string> = $state({});

  async function loadReports(refresh: boolean) {
    refreshing = true;
    try {
      reports = await api.usageReports(refresh);
    } catch {
      // The meter still works; the reports are an extra.
    }
    refreshing = false;
  }

  /** Save a usage key, after checking the provider takes it. */
  async function saveUsageKey(provider: string) {
    const key = keyInput[provider]?.trim();
    if (!key) return;
    keyNote[provider] = "Checking…";
    const t = await api.testKey(provider, key);
    if (t.result === "refused") {
      keyNote[provider] = "The provider refused this key.";
      return;
    }
    if (t.result === "failed") {
      keyNote[provider] = t.message;
      return;
    }
    try {
      await api.saveKey(provider, key);
      keyInput[provider] = "";
      keyNote[provider] = "";
      await loadReports(true);
    } catch (e) {
      keyNote[provider] = String(e);
    }
  }

  async function removeUsageKey(provider: string) {
    await api.saveKey(provider, "");
    await loadReports(true);
  }

  async function load() {
    const midnight = new Date();
    midnight.setHours(0, 0, 0, 0);
    try {
      usage = await api.usage(midnight.getTime());
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    load();
    loadReports(false);
    // Each paid request lands as a `usage` event: keep the numbers live.
    const off = onAppEvent((e) => e.kind === "usage" && load());
    const onFocus = () => load();
    addEventListener("focus", onFocus);
    return () => {
      removeEventListener("focus", onFocus);
      off.then((f) => f());
    };
  });

  async function reset() {
    await api.resetUsage();
    confirmReset = false;
    await load();
  }

  interface ProviderInfo {
    name: string;
    icon: Component<{ class?: string }>;
    console: string;
    speech: boolean;
  }
  const PROVIDERS: Record<string, ProviderInfo> = {
    deepgram: { name: "Deepgram", icon: AudioLines, console: "https://console.deepgram.com/", speech: true },
    anthropic: { name: "Claude", icon: Sparkles, console: "https://platform.claude.com/usage", speech: false },
    groq: { name: "Groq", icon: AudioLines, console: "https://console.groq.com/dashboard/usage", speech: true },
    sarvam: { name: "Sarvam", icon: AudioLines, console: "https://dashboard.sarvam.ai/", speech: true },
  };
  const info = (id: string): ProviderInfo =>
    PROVIDERS[id] ?? { name: id, icon: Gauge, console: "", speech: true };

  /** Deepgram and Claude always show (they're what Bolo pays for); others once used. */
  const rows = $derived.by(() => {
    const got = usage?.[period] ?? [];
    const out: ProviderUsage[] = ["deepgram", "anthropic"].map(
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
    return out.concat(got.filter((u) => u.provider !== "deepgram" && u.provider !== "anthropic"));
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

  const plural = (n: number, one: string, many: string) => `${count(n)} ${n === 1 ? one : many}`;
  const since = $derived(
    usage?.since_ms ? new Date(usage.since_ms).toLocaleDateString([], { day: "numeric", month: "long", year: "numeric" }) : null,
  );
</script>

<div class="flex items-center justify-between gap-3">
  <Segmented
    label="Period"
    value={period}
    options={[
      ["today", "Today"],
      ["last_30_days", "Last 30 days"],
      ["all_time", "All time"],
    ]}
    onchange={(v) => (period = v as Period)}
  />
  <div class="flex flex-col items-end" class:invisible={empty}>
    <span class="text-2xs text-muted-foreground">Estimated total</span>
    <span class="text-lg font-semibold tracking-[-0.01em] tabular-nums">{money(total)}</span>
  </div>
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if !usage && !error}
  <div class="flex flex-col gap-2.5" aria-hidden="true">
    {#each [0, 1] as i (i)}<div class="h-28 animate-pulse rounded-xl bg-muted"></div>{/each}
  </div>
{:else}
  {#each rows as r (r.provider)}
    {@const p = info(r.provider)}
    <SettingsSection title={p.name}>
      {#snippet action()}
        {#if p.console}
          <Button variant="ghost" size="sm" onclick={() => api.openLink(p.console)}>
            Dashboard <ExternalLink aria-hidden="true" />
          </Button>
        {/if}
      {/snippet}
      <div class="grid grid-cols-3 [&>*+*]:border-l [&>*+*]:border-border/60">
        {#if p.speech}
          <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
            <span class="text-xs text-muted-foreground">Audio sent</span>
            <span class="text-xl font-semibold tracking-[-0.02em] text-foreground tabular-nums">{duration(r.audio_secs)}</span>
            <span class="truncate text-2xs text-muted-foreground">{plural(r.requests, "request", "requests")}</span>
          </div>
          <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
            <span class="text-xs text-muted-foreground">Average</span>
            <span class="text-xl font-semibold tracking-[-0.02em] text-foreground tabular-nums">{r.requests ? duration(r.audio_secs / r.requests) : "–"}</span>
            <span class="truncate text-2xs text-muted-foreground">per request</span>
          </div>
        {:else}
          <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
            <span class="text-xs text-muted-foreground">Polish calls</span>
            <span class="text-xl font-semibold tracking-[-0.02em] text-foreground tabular-nums">{count(r.requests)}</span>
            <span class="truncate text-2xs text-muted-foreground">{count(r.input_tokens + r.cache_read_tokens + r.cache_write_tokens)} in · {count(r.output_tokens)} out</span>
          </div>
          <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
            <span class="text-xs text-muted-foreground">Tokens</span>
            <span class="text-xl font-semibold tracking-[-0.02em] text-foreground tabular-nums">{count(r.input_tokens + r.cache_read_tokens + r.cache_write_tokens + r.output_tokens)}</span>
            <span class="truncate text-2xs text-muted-foreground">{count(r.cache_read_tokens)} from the prompt cache</span>
          </div>
        {/if}
        <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
          <span class="text-xs text-muted-foreground">Estimated cost</span>
          <span class="text-xl font-semibold tracking-[-0.02em] text-foreground tabular-nums">{money(r.cost_usd)}</span>
          <span class="truncate text-2xs text-muted-foreground">{r.requests ? `${money(r.cost_usd / r.requests)} each` : "at list price"}</span>
        </div>
      </div>
      {#if r.provider === "deepgram" || r.provider === "anthropic"}
        {@render fromProvider(r.provider)}
      {/if}
    </SettingsSection>
  {/each}
{/if}

<div class="flex items-start justify-between gap-4 px-1">
  <p class="max-w-md text-xs text-muted-foreground">
    Counted by Bolo on this PC{since ? ` since ${since}` : ""}, from what each provider reported back for each request:
    Deepgram’s audio length, Claude’s tokens. Costs are at pay-as-you-go list prices. A usage key adds the provider’s own
    totals, which include other apps, and Deepgram’s credit left.
  </p>
  {#if !empty && usage}
    {#if confirmReset}
      <div class="flex shrink-0 items-center gap-1.5">
        <Button variant="ghost-muted" size="sm" onclick={() => (confirmReset = false)}>Cancel</Button>
        <Button variant="destructive-outline" size="sm" onclick={reset}>Reset</Button>
      </div>
    {:else}
      <Button variant="ghost-muted" size="sm" onclick={() => (confirmReset = true)}>
        <RotateCcw aria-hidden="true" />Reset…
      </Button>
    {/if}
  {/if}
</div>


{#snippet fromProvider(provider: string)}
  {@const id = provider === "deepgram" ? "deepgram-admin" : "anthropic-admin"}
  {@const report = provider === "deepgram" ? reports?.deepgram : reports?.anthropic}
  <div class="flex flex-col gap-2 bg-muted/40 px-4 py-3 dark:bg-white/[0.015]">
    {#if report?.state === "ok"}
      <div class="flex items-center justify-between gap-3">
        <span class="text-xs font-medium text-foreground">
          From {provider === "deepgram" ? "Deepgram" : "Anthropic"}
          <span class="font-normal text-muted-foreground">
            · {period === "today" ? "today (UTC)" : period === "last_30_days" ? "last 30 days" : provider === "anthropic" ? "last 12 months" : "all time"}
          </span>
        </span>
        <span class="flex items-center gap-1">
          <Button variant="ghost-muted" size="icon-xs" title="Refresh" disabled={refreshing} onclick={() => loadReports(true)}>
            <RefreshCw class={refreshing ? "animate-spin" : ""} aria-hidden="true" />
          </Button>
          <Button variant="ghost-muted" size="xs" onclick={() => removeUsageKey(id)}>Remove key</Button>
        </span>
      </div>
      {#if reports?.deepgram.state === "ok" && provider === "deepgram"}
        {@const d = reports.deepgram.report}
        {@const f = d.periods[period]}
        <p class="text-[13px] text-foreground tabular-nums">
          {hours(f.hours)} of audio · {count(Math.round(f.requests))} requests{f.usd !== null ? ` · ${money(f.usd)} charged` : ""}
          {#if d.balance !== null}
            · <span class="font-medium">{money(d.balance)} credit left</span>
          {/if}
        </p>
        <p class="text-2xs text-muted-foreground">The whole “{d.project}” project: every app and key in it.</p>
      {:else if reports?.anthropic.state === "ok" && provider === "anthropic"}
        {@const a = reports.anthropic.report}
        {@const f = a.periods[period]}
        <p class="text-[13px] text-foreground tabular-nums">
          {count(f.input_tokens + f.cache_read_tokens + f.cache_write_tokens)} tokens in · {count(f.output_tokens)} out
          · <span class="font-medium">{money(a.account_usd[period])}</span> charged to the account
        </p>
        <p class="text-2xs text-muted-foreground">
          {a.key_name
            ? `Tokens for Bolo’s key “${a.key_name}”; the charge is the whole account’s.`
            : "Bolo’s key wasn’t found in this organization, so the tokens are every key’s."}
        </p>
      {/if}
    {:else}
      <div class="flex items-center gap-2">
        <KeyRound class="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
        <span class="text-xs text-muted-foreground">
          {#if report?.state === "refused" || report?.state === "failed"}
            {report.message}
          {:else if provider === "deepgram"}
            Optional: a Deepgram key with the Admin role adds Deepgram’s own totals and your credit left.
          {:else}
            Optional: an Anthropic Admin key (organizations only) adds Anthropic’s own report.
          {/if}
        </span>
      </div>
      <div class="flex items-center gap-1.5">
        <label for="usage-key-{provider}" class="sr-only">{provider === "deepgram" ? "Deepgram" : "Anthropic"} usage key</label>
        <input
          id="usage-key-{provider}"
          class="input h-7! flex-1 text-[13px]!"
          type="password"
          placeholder={provider === "deepgram" ? "Paste a Deepgram Admin-role key…" : "Paste an sk-ant-admin01-… key…"}
          autocomplete="off"
          spellcheck="false"
          bind:value={keyInput[id]}
          onkeydown={(e) => e.key === "Enter" && saveUsageKey(id)}
        />
        <Button variant="outline" size="sm" disabled={!keyInput[id]?.trim()} onclick={() => saveUsageKey(id)}>Add</Button>
        {#if report?.state === "refused" || report?.state === "failed"}
          <Button variant="ghost-muted" size="sm" onclick={() => removeUsageKey(id)}>Remove</Button>
        {/if}
      </div>
      {#if keyNote[id]}<p class="text-2xs text-muted-foreground">{keyNote[id]}</p>{/if}
    {/if}
  </div>
{/snippet}
