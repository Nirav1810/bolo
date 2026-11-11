<script lang="ts">
  // First-run onboarding (M3 ticket 10, design approved in ticket 04): welcome → mic
  // check → model → try it. Each step is saved, so quitting partway resumes there.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    api,
    keyLabel,
    megabytes,
    meter,
    onDownload,
    onMicLevel,
    type DownloadEvent,
    type MicCheck,
    type SettingsView,
  } from "../lib/api";
  import { onAppEvent } from "../lib/events";

  let { ondone }: { ondone: () => void } = $props();

  const STEPS = 4;
  let step = $state(0);
  let view = $state<SettingsView | null>(null);

  // Mic check.
  let checking = $state(false);
  let level = $state(0);
  let mic = $state<MicCheck | null>(null);
  let micError = $state("");

  // Model.
  let download = $state<DownloadEvent | null>(null);
  let started = 0;
  let modelError = $state("");
  let cloudKey = $state("");
  let cloudError = $state("");

  // Try it.
  let practice = $state("");
  let latency = $state<number | null>(null);
  let tryError = $state("");

  const model = $derived(view?.models.find((m) => m.recommended) ?? null);
  const primary = $derived(view?.form.engines[0] ?? "");
  const cloudReady = $derived(!!view && !primary.startsWith("local:") && !!view.keys.deepgram);
  const modelReady = $derived(!!model?.installed && primary === model.spec);
  const key = $derived(view ? keyLabel(view.form.push_to_talk) : "Caps Lock");

  async function load() {
    for (let attempt = 0; attempt < 20; attempt++) {
      try {
        view = await api.settings();
        return;
      } catch {
        await new Promise((r) => setTimeout(r, 250));
      }
    }
  }

  $effect(() => {
    load().then(() => {
      step = Math.min(view?.onboarding_step ?? 0, STEPS - 1);
      if (view?.downloading.length && model) download = { id: model.id, phase: "downloading", downloaded: 0, total: 0, error: null };
      if (step === 1) runMicCheck();
    });
    const unlisten = [
      onMicLevel((rms) => (level = rms)),
      onDownload(onDownloadEvent),
      onAppEvent((e) => {
        if (step !== 3) return;
        if (e.kind === "final" && e.output.plain.trim()) latency = e.output.timings.total_ms;
        if (e.kind === "error") tryError = e.message;
      }),
    ];
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });

  async function go(to: number) {
    step = to;
    await api.onboardingSave(to);
    if (to === 1 && !mic) runMicCheck();
  }

  async function runMicCheck() {
    checking = true;
    micError = "";
    mic = null;
    try {
      mic = await api.micCheck();
    } catch (e) {
      micError = String(e);
    }
    checking = false;
    level = 0;
  }

  async function onDownloadEvent(e: DownloadEvent) {
    if (e.id !== model?.id) return;
    download = e;
    if (e.phase === "failed") modelError = e.error ?? "The download failed.";
    if (e.phase === "done" || e.phase === "cancelled" || e.phase === "failed") {
      if (e.phase !== "failed") download = null;
      view = await api.settings();
      if (e.phase === "done") await useModel();
    }
  }

  async function startDownload() {
    if (!model) return;
    modelError = "";
    started = Date.now();
    download = { id: model.id, phase: "downloading", downloaded: 0, total: model.download_bytes, error: null };
    try {
      await api.download(model.id);
    } catch (e) {
      download = null;
      modelError = String(e);
    }
  }

  /** Put the downloaded model first in the engine chain. */
  async function useModel() {
    if (!view || !model) return;
    const engines = [model.spec, ...view.form.engines.filter((e) => e !== model.spec)];
    view = await api.save({ ...$state.snapshot(view.form), engines });
  }

  async function useCloud() {
    if (!view) return;
    cloudError = "";
    try {
      const tested = await api.testKey("deepgram", cloudKey);
      if (tested.result === "refused") {
        cloudError = "Deepgram refused this key.";
        return;
      }
      view.keys.deepgram = await api.saveKey("deepgram", cloudKey);
      const engines = ["deepgram-stream", ...view.form.engines.filter((e) => e !== "deepgram-stream")];
      view = await api.save({ ...$state.snapshot(view.form), engines });
      cloudKey = "";
    } catch (e) {
      cloudError = String(e);
    }
  }

  function percent(d: DownloadEvent): number {
    return d.total ? Math.round((d.downloaded / d.total) * 100) : 0;
  }

  function timeLeft(d: DownloadEvent): string {
    const secs = (Date.now() - started) / 1000;
    if (!started || secs < 2 || !d.downloaded) return "";
    const rate = d.downloaded / secs;
    const left = (d.total - d.downloaded) / rate;
    return left < 60 ? ` · about ${Math.max(1, Math.round(left))} s left` : ` · about ${Math.round(left / 60)} min left`;
  }

  async function finish() {
    await api.onboardingSave(null);
    await getCurrentWindow().hide();
    ondone();
  }
</script>

{#snippet keycap(label: string)}
  <span class="mx-0.5 inline-flex h-7 items-center rounded-[4px] border border-control-border bg-control px-2.5 text-caption font-semibold shadow-[0_1px_0_var(--bolo-control-border)]">{label}</span>
{/snippet}

<div class="flex h-screen flex-col bg-surface">
  <main class="mx-auto flex w-full max-w-[600px] flex-1 flex-col justify-center gap-4 overflow-y-auto px-8 py-6">
    {#if step === 0}
      <svg width="56" height="56" viewBox="0 0 40 40" fill="none" aria-hidden="true">
        <rect width="40" height="40" rx="10" fill="var(--bolo-accent)" />
        <rect x="16" y="9" width="8" height="14" rx="4" fill="var(--bolo-on-accent)" />
        <path d="M12 20a8 8 0 0016 0M20 28v4" stroke="var(--bolo-on-accent)" stroke-width="2" stroke-linecap="round" />
      </svg>
      <h1 class="font-display text-title">Talk in any app. Bolo types it.</h1>
      <p class="text-text-2">
        English, Hinglish, or both in one sentence. It runs on this PC, so your voice stays here unless you pick a cloud
        engine.
      </p>
      <p>
        Hold {@render keycap(key)}, speak, let go.
        {#if key === "Caps Lock"}<span class="caption ml-2">A quick tap still types capitals.</span>{/if}
      </p>
    {:else if step === 1}
      <h1 class="font-display text-subtitle">Check your microphone</h1>
      <p class="text-text-2">Say anything for three seconds. Bolo checks that it hears the very start of what you say.</p>
      <div class="card compact">
        <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <rect x="5.5" y="1.5" width="5" height="8" rx="2.5" stroke="currentColor" stroke-width="1.4" />
          <path d="M3 7.5a5 5 0 0010 0M8 12.5v2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
        </svg>
        <span class="flex-none">{mic?.device ?? "Default microphone"}</span>
        <div class="progress flex-1" style="height: 6px" aria-hidden="true">
          <div style:width="{meter(level) * 100}%" style="background: var(--bolo-ok); transition-duration: 60ms"></div>
        </div>
        <span class="caption flex-none" role="status">
          {checking ? (level > 0.003 ? "Hearing you" : "Listening…") : mic ? (mic.heard ? "Heard you" : "Nothing heard") : ""}
        </span>
      </div>
      {#if micError}
        <p class="note danger" role="alert"><span class="icon" aria-hidden="true">⚠</span>Couldn't open the microphone: {micError}</p>
      {:else if mic && mic.late}
        <div class="note warn" role="alert">
          <span class="icon" aria-hidden="true">⚠</span>
          <div class="flex flex-col gap-1.5">
            <span class="font-semibold">
              {mic.leading_ms === null ? "Your mic sent only silence" : `Your mic starts ${mic.leading_ms} ms late`}
            </span>
            <span class="caption">
              {mic.leading_ms === null
                ? "Check that it isn't muted, and that Windows lets apps use the microphone."
                : "Windows Audio enhancements are muting the first moment of sound, so your first word gets clipped. Turn them off: Sound settings › your microphone › Audio enhancements › Off."}
            </span>
            <div class="mt-1 flex gap-2">
              <button type="button" class="btn" onclick={() => api.openLink("ms-settings:sound")}>Open sound settings</button>
              <button type="button" class="btn" onclick={runMicCheck}>Check again</button>
            </div>
          </div>
        </div>
      {:else if mic && !mic.heard}
        <div class="note warn" role="alert">
          <span class="icon" aria-hidden="true">⚠</span>
          <div class="flex flex-col gap-1.5">
            <span>Bolo didn't hear anything. Is the right microphone the Windows default?</span>
            <div class="flex gap-2">
              <button type="button" class="btn" onclick={() => api.openLink("ms-settings:sound")}>Open sound settings</button>
              <button type="button" class="btn" onclick={runMicCheck}>Check again</button>
            </div>
          </div>
        </div>
      {:else if mic}
        <p class="flex items-center gap-2.5" role="status">
          <span style="color: var(--bolo-ok)" aria-hidden="true">✓</span>
          Bolo hears you from the first moment ({mic.leading_ms} ms).
        </p>
      {/if}
    {:else if step === 2}
      <h1 class="font-display text-subtitle">Get the speech model</h1>
      {#if model}
        <p class="text-text-2">
          Picked for this PC ({Math.round((view?.ram_mb ?? 0) / 1024)} GB RAM, {model.hinglish ? "English and Hinglish" : "English"}).
          It downloads once, then works offline.
        </p>
        <div class="card selected flex-col items-stretch gap-2.5">
          <div class="flex items-center gap-2">
            <span class="font-semibold">{model.name}</span>
            <span class="tag accent">Recommended</span>
            <span class="flex-1"></span>
            <span class="caption">{megabytes(model.download_bytes)}</span>
          </div>
          {#if model.installed}
            <p class="flex items-center gap-2" role="status"><span style="color: var(--bolo-ok)" aria-hidden="true">✓</span>Downloaded and ready.</p>
          {:else if download && download.phase !== "failed"}
            <div class="progress" style="height: 6px" role="progressbar" aria-label="Download progress" aria-valuenow={percent(download)} aria-valuemin="0" aria-valuemax="100">
              <div style:width="{download.phase === 'downloading' ? percent(download) : 100}%"></div>
            </div>
            <div class="flex justify-between gap-4">
              <span class="caption">
                {download.phase === "downloading" ? `${percent(download)}%${timeLeft(download)}` : download.phase === "verifying" ? "Checking the download…" : "Unpacking…"}
              </span>
              <span class="caption">Resumes if you close this window</span>
            </div>
            <div><button type="button" class="btn" onclick={() => api.cancelDownload(model.id)}>Pause</button></div>
          {:else}
            {#if modelError}<p class="caption" style="color: var(--bolo-danger)" role="alert">⚠ {modelError}</p>{/if}
            <div><button type="button" class="btn primary" onclick={startDownload}>{modelError ? "Try again" : "Download"}</button></div>
          {/if}
        </div>
      {:else}
        <p class="text-text-2">This PC doesn't have the memory for an offline model. Use a cloud engine instead.</p>
      {/if}
      <div class="card flex-col items-stretch gap-1">
        <span class="font-semibold">Or use a cloud engine</span>
        <span class="caption">No download. Audio goes to Deepgram. Needs an API key, stored in Windows Credential Manager.</span>
        {#if cloudReady}
          <p class="mt-1 flex items-center gap-2" role="status"><span style="color: var(--bolo-ok)" aria-hidden="true">✓</span>Using Deepgram.</p>
        {:else}
          <form
            class="mt-1 flex gap-2"
            onsubmit={(e) => {
              e.preventDefault();
              useCloud();
            }}
          >
            <label for="cloud-key" class="sr-only">Deepgram API key</label>
            <input id="cloud-key" class="input flex-1" type="password" autocomplete="off" placeholder="Deepgram API key" bind:value={cloudKey} />
            <button type="submit" class="btn" disabled={!cloudKey.trim()}>Use Deepgram</button>
          </form>
          {#if cloudError}<p class="caption" style="color: var(--bolo-danger)" role="alert">⚠ {cloudError}</p>{/if}
        {/if}
      </div>
    {:else}
      <h1 class="font-display text-subtitle">Try it</h1>
      <p class="text-text-2">Click in the box, hold {@render keycap(key)} and say “Hello, this is my first dictation.”</p>
      <label for="practice" class="sr-only">Practice box</label>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea id="practice" class="input h-28 resize-none py-2.5 text-[15px] leading-[22px]" autofocus bind:value={practice}></textarea>
      {#if latency !== null}
        <p class="flex items-center gap-2.5" role="status">
          <span style="color: var(--bolo-ok)" aria-hidden="true">✓</span>
          It works. Text in {Math.round(latency)} ms after you let go.
        </p>
      {:else if tryError}
        <p class="note danger" role="alert"><span class="icon" aria-hidden="true">⚠</span>{tryError}</p>
      {/if}
      <p class="caption">Bolo lives in the tray. Right-click its icon for settings, history and pause.</p>
    {/if}
  </main>

  <footer class="flex items-center gap-2 border-t border-border bg-bg px-6 py-4">
    <ol class="flex gap-1.5" aria-label="Step {step + 1} of {STEPS}">
      {#each Array(STEPS) as _, i}
        <li
          class="h-1.5 rounded-full transition-[width] duration-[180ms]"
          class:w-5={i === step}
          class:w-1.5={i !== step}
          style:background={i <= step ? "var(--bolo-accent)" : "var(--bolo-control-border)"}
        ></li>
      {/each}
    </ol>
    <span class="flex-1"></span>
    {#if step > 0}<button type="button" class="btn" onclick={() => go(step - 1)}>Back</button>{/if}
    {#if step === 0}
      <button type="button" class="btn primary" onclick={() => go(1)}>Get started</button>
    {:else if step === 1}
      <button type="button" class="btn" class:primary={!!mic && !mic.late && mic.heard} disabled={checking} onclick={() => go(2)}>
        {mic && (mic.late || !mic.heard) ? "Continue anyway" : "Continue"}
      </button>
    {:else if step === 2}
      <button type="button" class="btn primary" disabled={!modelReady && !cloudReady} onclick={() => go(3)}>Next</button>
    {:else}
      <button type="button" class="btn primary" onclick={finish}>Finish</button>
    {/if}
  </footer>
</div>
