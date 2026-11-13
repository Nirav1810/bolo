<script lang="ts">
  import { ArrowUp, Cloud, Cpu, Download, KeyRound, Plus, X } from "@lucide/svelte";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import IconTile from "../lib/ui/IconTile.svelte";
  import KeyField from "../lib/ui/KeyField.svelte";
  import Progress from "../lib/ui/Progress.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import {
    PROVIDER_NAMES,
    api,
    downloadCaption,
    downloadPercent,
    engineName,
    megabytes,
    onDownload,
    saveCheckedKey,
    sentence,
    type DownloadEvent,
    type Form,
    type SettingsView,
  } from "../lib/api";

  let {
    view = $bindable(),
    update,
  }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  type RowStatus = { tone: "success" | "warning" | "error" | "busy"; text: string };

  let downloads: Record<string, DownloadEvent> = $state({});
  let replacing: Record<string, boolean> = $state({});
  let keyStatus: Record<string, RowStatus | null> = $state({});
  let modelError = $state<{ title: string; detail: string } | null>(null);

  const chain = $derived(view.form.engines);
  const providerOf = (spec: string) => view.cloud.find((c) => c.spec === spec.split(":")[0])?.provider;
  const needsKey = (spec: string) => {
    const p = providerOf(spec);
    return p !== undefined && !view.keys[p];
  };
  const modelName = (id: string) => view.models.find((m) => m.id === id)?.name ?? id;

  $effect(() => {
    for (const id of view.downloading) {
      downloads[id] ??= { id, phase: "downloading", downloaded: 0, total: 0, error: null };
    }
    const unlisten = onDownload(async (e) => {
      downloads[e.id] = e;
      if (e.phase === "done" || e.phase === "failed" || e.phase === "cancelled") {
        if (e.phase === "failed") modelError = { title: `Couldn’t download ${modelName(e.id)}`, detail: sentence(e.error ?? "") };
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
    modelError = null;
    downloads[id] = { id, phase: "downloading", downloaded: 0, total: 0, error: null };
    try {
      await api.download(id);
    } catch (e) {
      delete downloads[id];
      modelError = { title: `Couldn’t download ${modelName(id)}`, detail: sentence(String(e)) };
    }
  }

  async function remove(id: string) {
    modelError = null;
    try {
      await api.removeModel(id);
      view.models = (await api.settings()).models;
    } catch (e) {
      modelError = { title: `Couldn’t delete ${modelName(id)}`, detail: sentence(String(e)) };
    }
  }

  /** Checks and saves a pasted key; resolves to what went wrong, for the KeyField. */
  async function saveKey(provider: string, key: string): Promise<string | null> {
    const r = await saveCheckedKey(provider, key);
    if (!r.saved) return r.error;
    view.keys[provider] = r.hint;
    replacing[provider] = false;
    keyStatus[provider] = r.note ? { tone: "warning", text: r.note } : { tone: "success", text: "The key works." };
    return null;
  }

  async function deleteKey(provider: string) {
    keyStatus[provider] = null;
    try {
      view.keys[provider] = await api.saveKey(provider, "");
    } catch (e) {
      keyStatus[provider] = { tone: "error", text: sentence(String(e)) };
    }
  }

  /** Checks the saved key with its provider. */
  async function check(provider: string) {
    keyStatus[provider] = { tone: "busy", text: "Checking…" };
    try {
      const t = await api.testKey(provider);
      keyStatus[provider] =
        t.result === "ok"
          ? { tone: "success", text: "The key works." }
          : t.result === "refused"
            ? { tone: "error", text: `${PROVIDER_NAMES[provider]} refused this key.` }
            : t.result === "untestable"
              ? { tone: "success", text: "This key is checked on your first dictation." }
              : { tone: "error", text: `Couldn’t check the key: ${sentence(t.message)}` };
    } catch (e) {
      keyStatus[provider] = { tone: "error", text: `Couldn’t check the key: ${sentence(String(e))}` };
    }
  }

  function chainStatus(spec: string): RowStatus | null {
    if (needsKey(spec)) return { tone: "warning", text: `Needs a ${PROVIDER_NAMES[providerOf(spec) ?? ""]} API key, under API keys below.` };
    if (spec.startsWith("local:") && !view.models.find((m) => m.spec === spec)?.installed) return { tone: "warning", text: "Not downloaded yet." };
    return null;
  }
</script>

<SettingsSection title="Engine chain" description="Bolo uses the primary engine and falls back to the next if it fails.">
  {#each chain as spec, i (spec)}
    {@const local = spec.startsWith("local:")}
    <SettingsRow title={engineName(spec, view)} status={chainStatus(spec)}>
      {#snippet leading()}<IconTile icon={local ? Cpu : Cloud} tone={i === 0 ? "primary" : "neutral"} />{/snippet}
      {#snippet badges()}
        <Badge variant={i === 0 ? "primary" : "neutral"}>{i === 0 ? "Primary" : `Fallback ${i}`}</Badge>
      {/snippet}
      {#snippet description()}<span class="font-mono">{spec}</span>{/snippet}
      {#snippet control()}
        {#if i > 0}
          <Button variant="outline" size="sm" onclick={() => makePrimary(spec)}><ArrowUp aria-hidden="true" />Make primary</Button>
        {/if}
        {#if chain.length > 1}
          <Button variant="quiet" size="icon-sm" aria-label="Remove {engineName(spec, view)} from the chain" onclick={() => removeFromChain(spec)}>
            <X aria-hidden="true" />
          </Button>
        {/if}
      {/snippet}
    </SettingsRow>
  {/each}
</SettingsSection>

<SettingsSection title="On this PC" description="Private and offline. Your audio never leaves this PC.">
  {#each view.models as m (m.id)}
    {@const d = downloads[m.id]}
    {@const inUse = chain.includes(m.spec)}
    <SettingsRow title={m.name}>
      {#snippet leading()}<IconTile icon={Cpu} />{/snippet}
      {#snippet badges()}
        {#if m.recommended}<Badge variant="primary">Recommended</Badge>{/if}
        {#if inUse}<Badge variant="success">In the chain</Badge>{:else if m.installed}<Badge variant="neutral">Downloaded</Badge>{/if}
      {/snippet}
      {#snippet description()}
        {m.hinglish ? "English and Hinglish" : m.languages.length > 1 ? "English and European languages" : "English only"}
        · {megabytes(m.download_bytes)}{m.disk_bytes ? ` · ${megabytes(m.disk_bytes)} on disk` : ""}
      {/snippet}
      {#snippet control()}
        {#if d && d.phase !== "failed"}
          <Button variant="ghost" size="sm" onclick={() => api.cancelDownload(m.id)}>Cancel</Button>
        {:else if !m.installed}
          <Button variant="outline" size="sm" onclick={() => download(m.id)}><Download aria-hidden="true" />{d ? "Try again" : "Download"}</Button>
        {:else if !inUse}
          <Button variant="outline" size="sm" onclick={() => makePrimary(m.spec)}>Make primary</Button>
          <ConfirmButton label="Delete {m.name}" onconfirm={() => remove(m.id)} />
        {/if}
      {/snippet}
      {#if d && d.phase !== "failed"}
        <Progress label="Downloading {m.name}" value={downloadPercent(d)} caption="{downloadCaption(d, m.download_bytes)} · Resumes if you close this window." />
      {/if}
    </SettingsRow>
  {/each}
  {#snippet footer()}
    {#if modelError}<Alert variant="error" title={modelError.title}>{modelError.detail}</Alert>{/if}
  {/snippet}
</SettingsSection>

<SettingsSection title="Cloud" description="Your audio is sent to the provider. Each needs its own API key.">
  {#each view.cloud.filter((c) => !chain.includes(c.spec)) as c (c.spec)}
    {@const ready = !!view.keys[c.provider]}
    <SettingsRow title={c.name} description={ready ? "Ready to use." : `Needs a ${PROVIDER_NAMES[c.provider]} API key, under API keys below.`}>
      {#snippet leading()}<IconTile icon={Cloud} />{/snippet}
      {#snippet control()}
        <Button variant="ghost" size="sm" disabled={!ready} onclick={() => addFallback(c.spec)}><Plus aria-hidden="true" />Add as fallback</Button>
        <Button variant="outline" size="sm" disabled={!ready} onclick={() => makePrimary(c.spec)}>Make primary</Button>
      {/snippet}
    </SettingsRow>
  {:else}
    <SettingsRow title="Every cloud engine is in the chain" description="Remove one from the engine chain to see it here again.">
      {#snippet leading()}<IconTile icon={Cloud} />{/snippet}
    </SettingsRow>
  {/each}
</SettingsSection>

<SettingsSection title="API keys" description="Stored in Windows Credential Manager under “bolo”. Never written to a file.">
  {#each Object.keys(PROVIDER_NAMES) as p (p)}
    {@const hint = view.keys[p]}
    {@const saved = hint !== null && hint !== undefined && !replacing[p]}
    <SettingsRow title={PROVIDER_NAMES[p]} labelFor={saved ? undefined : `key-${p}`} status={keyStatus[p] ?? null}>
      {#snippet leading()}<IconTile icon={KeyRound} tone={saved ? "success" : "neutral"} />{/snippet}
      {#snippet badges()}{#if saved}<Badge variant="success">Saved</Badge>{/if}{/snippet}
      {#snippet description()}
        {#if saved}<span class="font-mono">•••• •••• {hint}</span>{:else}{p === "anthropic" ? "For polish." : "Optional, for its cloud engine."}{/if}
      {/snippet}
      {#snippet control()}
        {#if saved}
          <Button variant="ghost" size="sm" onclick={() => check(p)}>Check</Button>
          <Button
            variant="outline"
            size="sm"
            onclick={() => {
              replacing[p] = true;
              keyStatus[p] = null;
            }}>Replace…</Button
          >
          <ConfirmButton label="Delete the {PROVIDER_NAMES[p]} API key" onconfirm={() => deleteKey(p)} />
        {:else}
          <div class="w-64">
            <KeyField
              id="key-{p}"
              disabled={!view.keystore}
              onsave={(key) => saveKey(p, key)}
              oncancel={replacing[p] ? () => (replacing[p] = false) : undefined}
            />
          </div>
        {/if}
      {/snippet}
    </SettingsRow>
  {/each}
  {#snippet footer()}
    {#if !view.keystore}
      <Alert variant="warning" title="API keys can’t be saved">Windows Credential Manager couldn’t be opened.</Alert>
    {/if}
  {/snippet}
</SettingsSection>
