<script lang="ts">
  import { Cloud, Cpu, Download, KeyRound, Trash2, X, TriangleAlert, ArrowUp, Plus } from "@lucide/svelte";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import {
    PROVIDER_NAMES,
    api,
    engineName,
    megabytes,
    onDownload,
    type DownloadEvent,
    type Form,
    type SettingsView,
    type Tested,
  } from "../lib/api";

  let {
    view = $bindable(),
    update,
  }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let downloads: Record<string, DownloadEvent> = $state({});
  let keyInput: Record<string, string> = $state({});
  let replacing: Record<string, boolean> = $state({});
  let tested: Record<string, string> = $state({});
  let keyError = $state("");
  let modelError = $state("");

  const chain = $derived(view.form.engines);
  const providerOf = (spec: string) => view.cloud.find((c) => c.spec === spec.split(":")[0])?.provider;
  const needsKey = (spec: string) => {
    const p = providerOf(spec);
    return p !== undefined && !view.keys[p];
  };

  $effect(() => {
    for (const id of view.downloading) {
      downloads[id] ??= { id, phase: "downloading", downloaded: 0, total: 0, error: null };
    }
    const unlisten = onDownload(async (e) => {
      downloads[e.id] = e;
      if (e.phase === "done" || e.phase === "failed" || e.phase === "cancelled") {
        if (e.phase === "failed") modelError = `${e.id}: ${e.error}`;
        const fresh = await api.settings();
        view.models = fresh.models;
        view.downloading = fresh.downloading;
        if (e.phase !== "failed") delete downloads[e.id];
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  function makePrimary(spec: string) {
    update((f) => (f.engines = [spec, ...f.engines.filter((e) => e !== spec)]));
  }
  function addFallback(spec: string) {
    update((f) => (f.engines = [...f.engines.filter((e) => e !== spec), spec]));
  }
  function removeFromChain(spec: string) {
    update((f) => (f.engines = f.engines.filter((e) => e !== spec)));
  }

  async function download(id: string) {
    modelError = "";
    downloads[id] = { id, phase: "downloading", downloaded: 0, total: 0, error: null };
    try {
      await api.download(id);
    } catch (e) {
      delete downloads[id];
      modelError = String(e);
    }
  }

  async function remove(id: string) {
    modelError = "";
    try {
      await api.removeModel(id);
      view.models = (await api.settings()).models;
    } catch (e) {
      modelError = String(e);
    }
  }

  async function saveKey(provider: string, key: string) {
    keyError = "";
    try {
      view.keys[provider] = await api.saveKey(provider, key);
      keyInput[provider] = "";
      replacing[provider] = false;
      tested[provider] = "";
    } catch (e) {
      keyError = `${PROVIDER_NAMES[provider]}: ${e}`;
    }
  }

  function describe(t: Tested): string {
    switch (t.result) {
      case "ok":
        return "The key works.";
      case "refused":
        return "The provider refused this key.";
      case "untestable":
        return "This key is checked on your first dictation.";
      case "failed":
        return `Couldn't check: ${t.message}`;
    }
  }

  async function test(provider: string) {
    tested[provider] = "Checking…";
    tested[provider] = describe(await api.testKey(provider, keyInput[provider]));
  }

  function percent(d: DownloadEvent): number {
    return d.total ? Math.round((d.downloaded / d.total) * 100) : 0;
  }
</script>

{#snippet tile(Icon: typeof Cpu)}
  <div class="grid size-8 place-items-center rounded-lg border border-border/70 bg-muted">
    <Icon class="size-4 text-muted-foreground" aria-hidden="true" />
  </div>
{/snippet}

<SettingsSection title="Engine chain" description="Bolo uses the first engine and falls back to the next if it fails.">
  {#each chain as spec, i (spec)}
    {@const local = spec.startsWith("local:")}
    <SettingsRow title={engineName(spec, view)}>
      {#snippet leading()}{@render tile(local ? Cpu : Cloud)}{/snippet}
      {#snippet badges()}
        <Badge variant={i === 0 ? "primary" : "secondary"}>{i === 0 ? "Primary" : `Fallback ${i}`}</Badge>
      {/snippet}
      {#snippet description()}<span class="mono text-2xs">{spec}</span>{/snippet}
      {#snippet status()}
        {#if needsKey(spec)}
          <span class="inline-flex items-center gap-1 text-warning-foreground"
            ><TriangleAlert class="size-3.5" aria-hidden="true" />Needs a {PROVIDER_NAMES[providerOf(spec) ?? ""]} key below.</span
          >
        {:else if local && !view.models.find((m) => m.spec === spec)?.installed}
          <span class="inline-flex items-center gap-1 text-warning-foreground"
            ><TriangleAlert class="size-3.5" aria-hidden="true" />Not downloaded yet.</span
          >
        {/if}
      {/snippet}
      {#snippet control()}
        {#if i > 0}
          <Button variant="outline" size="sm" onclick={() => makePrimary(spec)}><ArrowUp aria-hidden="true" />Make primary</Button>
        {/if}
        {#if chain.length > 1}
          <Button
            variant="ghost-destructive"
            size="icon-sm"
            aria-label="Remove {engineName(spec, view)}"
            title="Remove"
            onclick={() => removeFromChain(spec)}
          >
            <X aria-hidden="true" />
          </Button>
        {/if}
      {/snippet}
    </SettingsRow>
  {/each}
</SettingsSection>

<SettingsSection title="On this PC" description="Private and offline. Your audio never leaves the PC.">
  {#each view.models as m (m.id)}
    {@const d = downloads[m.id]}
    {@const inUse = chain.includes(m.spec)}
    <SettingsRow title={m.name}>
      {#snippet leading()}{@render tile(Cpu)}{/snippet}
      {#snippet badges()}
        {#if m.recommended}<Badge variant="primary">Recommended</Badge>{/if}
        {#if inUse}<Badge variant="success">In use</Badge>{:else if m.installed}<Badge variant="secondary">Downloaded</Badge>{/if}
      {/snippet}
      {#snippet description()}
        {m.hinglish ? "English and Hinglish" : m.languages.length > 1 ? "English and European languages" : "English only"}
        · {megabytes(m.download_bytes)}{m.disk_bytes ? `, ${megabytes(m.disk_bytes)} on disk` : ""}
      {/snippet}
      {#snippet control()}
        {#if d && d.phase !== "failed"}
          <Button variant="ghost-muted" size="sm" onclick={() => api.cancelDownload(m.id)}>Cancel</Button>
        {:else if !m.installed}
          <Button variant="outline" size="sm" onclick={() => download(m.id)}><Download aria-hidden="true" />{d ? "Retry" : "Download"}</Button>
        {:else if !inUse}
          <Button variant="outline" size="sm" onclick={() => makePrimary(m.spec)}>Use</Button>
          <ConfirmButton label="Delete {m.name}" onconfirm={() => remove(m.id)}><Trash2 aria-hidden="true" /></ConfirmButton>
        {/if}
      {/snippet}
      {#if d && d.phase !== "failed"}
        <div class="ml-11 flex flex-col gap-1.5">
          <div
            class="progress"
            role="progressbar"
            aria-valuenow={percent(d)}
            aria-valuemin="0"
            aria-valuemax="100"
            aria-label="Downloading {m.name}"
          >
            <div style:width="{d.phase === 'downloading' ? percent(d) : 100}%"></div>
          </div>
          <span class="text-2xs text-muted-foreground tabular-nums">
            {d.phase === "downloading"
              ? `${megabytes(d.downloaded)} of ${megabytes(d.total || m.download_bytes)}`
              : d.phase === "verifying"
                ? "Checking the download…"
                : "Unpacking…"}
            · Resumes if you close this window
          </span>
        </div>
      {/if}
    </SettingsRow>
  {/each}
</SettingsSection>
{#if modelError}<Alert variant="error" class="-mt-5">{modelError}</Alert>{/if}

<SettingsSection title="Cloud" description="Your audio is sent to the provider. Each needs its own key.">
  {#each view.cloud.filter((c) => !chain.includes(c.spec)) as c (c.spec)}
    {@const ready = !!view.keys[c.provider]}
    <SettingsRow title={c.name} description={ready ? "Key saved." : `Needs a ${PROVIDER_NAMES[c.provider]} key.`}>
      {#snippet leading()}{@render tile(Cloud)}{/snippet}
      {#snippet control()}
        <Button variant="ghost" size="sm" disabled={!ready} onclick={() => addFallback(c.spec)}><Plus aria-hidden="true" />Fallback</Button>
        <Button variant="outline" size="sm" disabled={!ready} onclick={() => makePrimary(c.spec)}>Use as primary</Button>
      {/snippet}
    </SettingsRow>
  {:else}
    <p class="px-4 py-3 text-xs text-muted-foreground">Every cloud engine is already in the chain.</p>
  {/each}
</SettingsSection>

<SettingsSection title="API keys" description="Stored in Windows Credential Manager under “bolo”. Never written to a file.">
  {#each Object.keys(PROVIDER_NAMES) as p (p)}
    {@const hint = view.keys[p]}
    {@const saved = hint !== null && hint !== undefined && !replacing[p]}
    <SettingsRow title={PROVIDER_NAMES[p]} labelFor={saved ? undefined : `key-${p}`}>
      {#snippet leading()}{@render tile(KeyRound)}{/snippet}
      {#snippet badges()}{#if saved}<Badge variant="success">Saved</Badge>{/if}{/snippet}
      {#snippet description()}
        {#if saved}<span class="mono text-2xs">•••• •••• {hint}</span>{:else}{p === "anthropic"
            ? "For polish."
            : "Optional, for its cloud engine."}{/if}
      {/snippet}
      {#snippet status()}{#if tested[p]}<span class="text-muted-foreground" role="status">{tested[p]}</span>{/if}{/snippet}
      {#snippet control()}
        {#if saved}
          <Button variant="ghost" size="sm" onclick={() => test(p)}>Test</Button>
          <Button variant="outline" size="sm" onclick={() => (replacing[p] = true)}>Replace</Button>
          <ConfirmButton label="Remove the {PROVIDER_NAMES[p]} key" confirm="Remove?" onconfirm={() => saveKey(p, "")}>
            <Trash2 aria-hidden="true" />
          </ConfirmButton>
        {:else}
          <input
            id="key-{p}"
            class="input mono h-7! w-48! text-xs!"
            type="password"
            placeholder="Paste key…"
            autocomplete="off"
            spellcheck="false"
            disabled={!view.keystore}
            bind:value={keyInput[p]}
            onkeydown={(e) => e.key === "Enter" && keyInput[p]?.trim() && saveKey(p, keyInput[p])}
          />
          {#if replacing[p]}<Button variant="ghost-muted" size="sm" onclick={() => (replacing[p] = false)}>Cancel</Button>{/if}
          <Button variant="ghost" size="sm" disabled={!keyInput[p]?.trim()} onclick={() => test(p)}>Test</Button>
          <Button variant="primary" size="sm" disabled={!keyInput[p]?.trim()} onclick={() => saveKey(p, keyInput[p])}>Save</Button>
        {/if}
      {/snippet}
    </SettingsRow>
  {/each}
</SettingsSection>
{#if !view.keystore}
  <Alert variant="warning" class="-mt-5" title="Keys can’t be saved">Windows Credential Manager couldn’t be opened.</Alert>
{/if}
{#if keyError}<Alert variant="error" class="-mt-5">{keyError}</Alert>{/if}
