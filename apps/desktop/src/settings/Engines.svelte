<script lang="ts">
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

<h2 class="section mt-0">In use</h2>
<p class="caption -mt-1 mb-2">Bolo uses the first engine and falls back to the next if it fails.</p>
<div class="group">
  {#each chain as spec, i (spec)}
    <div class="card compact" class:selected={i === 0}>
      <div class="card-text">
        <span class="flex items-center gap-2">
          {engineName(spec, view)}
          <span class="tag" class:ok={i === 0}>{i === 0 ? "Primary" : `Fallback ${i}`}</span>
        </span>
        <span class="caption mono">{spec}</span>
        {#if needsKey(spec)}
          <span class="caption" style="color: var(--bolo-warn)">⚠ Needs a {PROVIDER_NAMES[providerOf(spec) ?? ""]} key below.</span>
        {:else if spec.startsWith("local:") && !view.models.find((m) => m.spec === spec)?.installed}
          <span class="caption" style="color: var(--bolo-warn)">⚠ Not downloaded yet.</span>
        {/if}
      </div>
      {#if i > 0}<button type="button" class="btn" onclick={() => makePrimary(spec)}>Make primary</button>{/if}
      {#if chain.length > 1}<button type="button" class="btn" onclick={() => removeFromChain(spec)}>Remove</button>{/if}
    </div>
  {/each}
</div>

<h2 class="section">On this PC</h2>
<p class="caption -mt-1 mb-2">Private and offline. Your audio never leaves the PC.</p>
{#if modelError}<p class="note danger mb-2" role="alert"><span class="icon" aria-hidden="true">⚠</span>{modelError}</p>{/if}
<div class="group">
  {#each view.models as m (m.id)}
    {@const d = downloads[m.id]}
    <div class="card">
      <div class="card-text">
        <span class="flex items-center gap-2">
          {m.name}
          {#if m.recommended}<span class="tag accent">Recommended</span>{/if}
          {#if m.installed}<span class="tag ok">Downloaded</span>{/if}
        </span>
        <span class="caption">
          {m.hinglish ? "English and Hinglish" : m.languages.length > 1 ? "English and European languages" : "English only"}
          · {megabytes(m.download_bytes)} download{m.disk_bytes ? `, ${megabytes(m.disk_bytes)} on disk` : ""}
        </span>
        {#if d && d.phase !== "failed"}
          <div class="progress mt-1.5" role="progressbar" aria-valuenow={percent(d)} aria-valuemin="0" aria-valuemax="100" aria-label="Downloading {m.name}">
            <div style:width="{d.phase === 'downloading' ? percent(d) : 100}%"></div>
          </div>
          <span class="caption">
            {d.phase === "downloading" ? `${megabytes(d.downloaded)} of ${megabytes(d.total || m.download_bytes)}` : d.phase === "verifying" ? "Checking the download…" : "Unpacking…"}
            · Resumes if you close this window.
          </span>
        {/if}
      </div>
      {#if d && d.phase !== "failed"}
        <button type="button" class="btn" onclick={() => api.cancelDownload(m.id)}>Cancel</button>
      {:else if !m.installed}
        <button type="button" class="btn" onclick={() => download(m.id)}>{d ? "Retry" : "Download"}</button>
      {:else if !chain.includes(m.spec)}
        <button type="button" class="btn" onclick={() => makePrimary(m.spec)}>Use</button>
        <button type="button" class="btn danger" onclick={() => remove(m.id)}>Delete</button>
      {/if}
    </div>
  {/each}
</div>

<h2 class="section">Cloud</h2>
<p class="caption -mt-1 mb-2">Your audio is sent to the provider. Each needs its own key.</p>
<div class="group">
  {#each view.cloud as c (c.spec)}
    {#if !chain.includes(c.spec)}
      <div class="card compact">
        <div class="card-text">
          <span>{c.name}</span>
          <span class="caption">{view.keys[c.provider] ? "Key saved." : `Needs a ${PROVIDER_NAMES[c.provider]} key.`}</span>
        </div>
        <button type="button" class="btn" disabled={!view.keys[c.provider]} onclick={() => makePrimary(c.spec)}>Use as primary</button>
        <button type="button" class="btn" disabled={!view.keys[c.provider]} onclick={() => addFallback(c.spec)}>Add as fallback</button>
      </div>
    {/if}
  {/each}
</div>

<h2 class="section">API keys</h2>
<p class="caption -mt-1 mb-2">Stored in Windows Credential Manager under “bolo”. Never written to a file.</p>
{#if !view.keystore}
  <p class="note warn mb-2"><span class="icon" aria-hidden="true">⚠</span>Windows Credential Manager couldn't be opened, so keys can't be saved.</p>
{/if}
{#if keyError}<p class="note danger mb-2" role="alert"><span class="icon" aria-hidden="true">⚠</span>{keyError}</p>{/if}
<div class="group">
  {#each Object.keys(PROVIDER_NAMES) as p (p)}
    {@const hint = view.keys[p]}
    <div class="card">
      <div class="card-text">
        <label for="key-{p}">{PROVIDER_NAMES[p]}</label>
        {#if hint !== null && hint !== undefined && !replacing[p]}
          <span class="caption mono">•••••••••••• {hint}</span>
        {:else}
          <span class="caption">{p === "anthropic" ? "For polish." : "Optional."}</span>
        {/if}
        {#if tested[p]}<span class="caption" role="status">{tested[p]}</span>{/if}
      </div>
      {#if hint !== null && hint !== undefined && !replacing[p]}
        <span class="tag ok">Saved</span>
        <button type="button" class="btn" onclick={() => test(p)}>Test</button>
        <button type="button" class="btn" onclick={() => (replacing[p] = true)}>Replace</button>
        <button type="button" class="btn danger" onclick={() => saveKey(p, "")}>Remove</button>
      {:else}
        <input
          id="key-{p}"
          class="input w-56"
          type="password"
          placeholder="Paste key"
          autocomplete="off"
          disabled={!view.keystore}
          bind:value={keyInput[p]}
          onkeydown={(e) => e.key === "Enter" && keyInput[p]?.trim() && saveKey(p, keyInput[p])}
        />
        <button type="button" class="btn" disabled={!keyInput[p]?.trim()} onclick={() => test(p)}>Test</button>
        <button type="button" class="btn primary" disabled={!keyInput[p]?.trim()} onclick={() => saveKey(p, keyInput[p])}>Save</button>
        {#if replacing[p]}<button type="button" class="btn" onclick={() => (replacing[p] = false)}>Cancel</button>{/if}
      {/if}
    </div>
  {/each}
</div>
