<script lang="ts">
  // The usage meter: what each paid cloud request was billed for, as the provider
  // reported it (audio length, Claude token counts), recorded per request by Rust
  // (src-tauri/src/journal.rs) and priced at list price. An optional admin key adds
  // the provider's own account totals (src-tauri/src/billing.rs).
  import { AudioLines, ExternalLink, Gauge, RefreshCw, RotateCcw, Sparkles } from "@lucide/svelte";
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
  /** Usage-key fields the user opened. */
  let addingKey: Record<string, boolean> = $state({});
  /** Provider → hint of the key stored on Engines & keys (null: none). */
  let storedKeys: Record<string, string | null> = $state({});

  async function loadKeys() {
    try {
      storedKeys = (await api.settings()).keys;
    } catch {
      // Only used for the hints under each card.
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
      addingKey[provider] = false;
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

  const hasReport = (provider: string) =>
    (provider === "deepgram" ? reports?.deepgram : reports?.anthropic)?.state === "ok";
  const source = (provider: string) => (provider === "deepgram" ? reports?.deepgram_key : reports?.anthropic_key);

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
{:else if rows.length === 0}
  <p class="px-1 py-6 text-center text-sm text-muted-foreground">
    No cloud usage yet. Add a key on Engines &amp; keys to use a cloud engine.
  </p>
{:else}
  {#each rows as r (r.provider)}
    {@const p = info(r.provider)}
    {@const adminId = r.provider === "deepgram" ? "deepgram-admin" : r.provider === "anthropic" ? "anthropic-admin" : null}
    <SettingsSection title={p.name}>
      {#snippet action()}
        <span class="flex items-center gap-1">
          {#if adminId && !hasReport(r.provider) && !addingKey[adminId] && source(r.provider) !== "usage"}
            <Button
              variant="ghost-muted"
              size="sm"
              title={r.provider === "deepgram"
                ? "A Deepgram Admin-role key shows the account’s own totals and credit left"
                : "An Anthropic Admin key (sk-ant-admin…) shows the account’s own totals"}
              onclick={() => (addingKey[adminId] = true)}>Add admin key</Button
            >
          {/if}
          {#if p.console}
            <Button variant="ghost" size="sm" onclick={() => api.openLink(p.console)}>
              Dashboard <ExternalLink aria-hidden="true" />
            </Button>
          {/if}
        </span>
      {/snippet}
      <div class="grid grid-cols-3 [&>*+*]:border-l [&>*+*]:border-border/60">
        {#if p.speech}
          {@render stat("Audio", duration(r.audio_secs), plural(r.requests, "request", "requests"))}
          {@render stat("Average", r.requests ? duration(r.audio_secs / r.requests) : "–", r.requests ? "per request" : "")}
        {:else}
          {@render stat("Polish calls", count(r.requests), "")}
          {@render stat(
            "Tokens",
            count(r.input_tokens + r.cache_read_tokens + r.cache_write_tokens + r.output_tokens),
            r.requests ? `${count(r.input_tokens + r.cache_read_tokens + r.cache_write_tokens)} in · ${count(r.output_tokens)} out` : "",
          )}
        {/if}
        {@render stat("Est. cost", money(r.cost_usd), r.requests ? `${money(r.cost_usd / r.requests)} each` : "")}
      </div>
      {#if adminId}{@render fromProvider(r.provider, adminId)}{/if}
    </SettingsSection>
  {/each}
{/if}

<div class="flex items-center justify-between gap-4 px-1">
  <p class="text-xs text-muted-foreground">Estimated at list prices{since ? `, since ${since}` : ""}.</p>
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

{#snippet stat(label: string, value: string, sub: string)}
  <div class="flex min-w-0 flex-col gap-0.5 px-4 py-3.5">
    <span class="text-xs text-muted-foreground">{label}</span>
    <span class="text-xl font-semibold tracking-[-0.02em] text-foreground tabular-nums">{value}</span>
    <span class="truncate text-2xs text-muted-foreground">{sub || " "}</span>
  </div>
{/snippet}

{#snippet fromProvider(provider: string, id: string)}
  {@const report = provider === "deepgram" ? reports?.deepgram : reports?.anthropic}
  {@const src = source(provider)}
  {@const message = src === "usage" && (report?.state === "refused" || report?.state === "failed") ? report.message : ""}
  {#if report?.state === "ok" || addingKey[id] || src === "usage" || keyNote[id]}
    <div class="flex flex-col gap-2 bg-muted/40 px-4 py-3 dark:bg-white/[0.015]">
      {#if report?.state === "ok"}
        <div class="flex items-center justify-between gap-3">
          <p class="text-[13px] text-foreground tabular-nums">
            <span class="text-muted-foreground"
              >Account{period === "today" ? " (today, UTC)" : period === "all_time" && provider === "anthropic" ? " (12 months)" : ""}:</span
            >
            {#if reports?.deepgram.state === "ok" && provider === "deepgram"}
              {@const d = reports.deepgram.report}
              {@const f = d.periods[period]}
              {hours(f.hours)} · {count(Math.round(f.requests))} requests{f.usd !== null ? ` · ${money(f.usd)}` : ""}
              {#if d.balance !== null}· <span class="font-medium">{money(d.balance)} credit left</span>{/if}
            {:else if reports?.anthropic.state === "ok" && provider === "anthropic"}
              {@const a = reports.anthropic.report}
              {@const f = a.periods[period]}
              {count(f.input_tokens + f.cache_read_tokens + f.cache_write_tokens)} in · {count(f.output_tokens)} out ·
              <span class="font-medium">{money(a.account_usd[period])}</span>
            {/if}
          </p>
          <span class="flex shrink-0 items-center gap-1">
            <Button variant="ghost-muted" size="icon-xs" title="Refresh" disabled={refreshing} onclick={() => loadReports(true)}>
              <RefreshCw class={refreshing ? "animate-spin" : ""} aria-hidden="true" />
            </Button>
            {#if src === "usage"}
              <Button variant="ghost-muted" size="xs" onclick={() => removeUsageKey(id)}>Remove key</Button>
            {/if}
          </span>
        </div>
      {:else}
        {#if message}<p class="text-xs text-muted-foreground">{message}</p>{/if}
        <div class="flex items-center gap-1.5">
          <label for="usage-key-{provider}" class="sr-only">{provider === "deepgram" ? "Deepgram" : "Anthropic"} admin key</label>
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
          {#if src === "usage"}
            <Button variant="ghost-muted" size="sm" onclick={() => removeUsageKey(id)}>Remove</Button>
          {:else}
            <Button variant="ghost-muted" size="sm" onclick={() => ((addingKey[id] = false), (keyNote[id] = ""))}>Cancel</Button>
          {/if}
        </div>
        {#if keyNote[id]}<p class="text-2xs text-muted-foreground">{keyNote[id]}</p>{/if}
      {/if}
    </div>
  {/if}
{/snippet}
