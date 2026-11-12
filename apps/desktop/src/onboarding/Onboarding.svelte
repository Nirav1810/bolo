<script lang="ts">
  // First-run onboarding (M3 ticket 10, design approved in ticket 04): welcome → mic
  // check → model → try it. Each step is saved, so quitting partway resumes there.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    ArrowLeft,
    ArrowRight,
    AudioLines,
    Check,
    CircleAlert,
    CircleCheck,
    Cloud,
    Cpu,
    Download,
    ExternalLink,
    Keyboard,
    Mic,
    Pause,
    RefreshCw,
  } from "@lucide/svelte";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Kbd from "../lib/ui/Kbd.svelte";
  import releaseIcon from "../../src-tauri/icons/128x128.png?no-inline";
  import nightlyIcon from "../../src-tauri/icons-nightly/128x128.png?no-inline";
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
    variant,
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
  const nightly = $derived(!!view && variant(view.version).nightly);
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
    // Closing destroys the window and its webview; the tray builds it again.
    await getCurrentWindow().close();
    ondone();
  }
</script>

{#snippet stepIcon(Icon: typeof Mic)}
  <div class="grid size-11 place-items-center rounded-xl border border-border bg-card shadow-xs/5">
    <Icon class="size-5 text-foreground" aria-hidden="true" />
  </div>
{/snippet}

{#snippet ok(text: string)}
  <p class="flex items-center gap-2 text-sm text-foreground" role="status">
    <CircleCheck class="size-4 shrink-0 text-success-foreground" aria-hidden="true" />{text}
  </p>
{/snippet}

<div class="flex h-screen flex-col bg-background">
  <header class="flex items-center justify-between px-6 pt-5">
    <div class="flex items-center gap-2">
      <img src={nightly ? nightlyIcon : releaseIcon} width="22" height="22" alt="" class="rounded-md" />
      <span class="text-sm font-semibold tracking-[-0.01em]">{nightly ? "Bolo Nightly" : "Bolo"}</span>
    </div>
    <div class="flex items-center gap-3">
      <span class="text-xs text-muted-foreground tabular-nums">Step {step + 1} of {STEPS}</span>
      <ol class="flex gap-1" aria-label="Step {step + 1} of {STEPS}">
        {#each Array(STEPS) as _, i (i)}
          <li class="h-1 w-6 overflow-hidden rounded-full bg-accent dark:bg-input/60">
            <div class="h-full rounded-full bg-primary transition-[width] duration-300 ease-out-ui" style:width={i <= step ? "100%" : "0%"}></div>
          </li>
        {/each}
      </ol>
    </div>
  </header>

  <main class="flex flex-1 flex-col overflow-y-auto px-8">
    {#key step}
      <div class="enter mx-auto my-auto flex w-full max-w-[520px] flex-col gap-5 py-8">
        {#if step === 0}
          {@render stepIcon(AudioLines)}
          <div class="flex flex-col gap-2">
            <h1 class="text-2xl font-semibold tracking-[-0.025em]">Talk in any app. Bolo types it.</h1>
            <p class="text-[15px] leading-6 text-muted-foreground">
              English, Hinglish, or both in one sentence. It runs on this PC, so your voice stays here unless you pick a
              cloud engine.
            </p>
          </div>
          <ol class="overflow-hidden rounded-xl border border-border/80 bg-card shadow-xs/5 dark:border-border [&>*+*]:border-t [&>*+*]:border-border/60">
            {#each [["Hold", "the key, anywhere you can type"], ["Speak", "naturally, in English or Hinglish"], ["Let go", "and the text lands where your cursor is"]] as [verb, rest], i (verb)}
              <li class="flex items-center gap-3 px-4 py-3">
                <span class="grid size-6 shrink-0 place-items-center rounded-full bg-accent text-xs font-semibold text-muted-foreground tabular-nums">{i + 1}</span>
                <span class="text-sm">
                  <span class="font-medium">{verb}</span>
                  {#if i === 0}<Kbd class="mx-0.5">{key}</Kbd>{/if}
                  <span class="text-muted-foreground">{rest}</span>
                </span>
              </li>
            {/each}
          </ol>
          {#if key === "Caps Lock"}<p class="text-xs text-muted-foreground">A quick tap of Caps Lock still types capitals.</p>{/if}
        {:else if step === 1}
          {@render stepIcon(Mic)}
          <div class="flex flex-col gap-2">
            <h1 class="text-2xl font-semibold tracking-[-0.025em]">Check your microphone</h1>
            <p class="text-[15px] leading-6 text-muted-foreground">
              Say anything for three seconds. Bolo checks that it hears the very start of what you say.
            </p>
          </div>
          <div class="flex flex-col gap-3 rounded-xl border border-border/80 bg-card px-4 py-3.5 shadow-xs/5 dark:border-border">
            <div class="flex items-center gap-2">
              <Mic class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
              <span class="min-w-0 flex-1 truncate text-sm font-medium">{mic?.device ?? "Default microphone"}</span>
              {#if checking}
                <Badge variant={level > 0.003 ? "success" : "secondary"}>{level > 0.003 ? "Hearing you" : "Listening…"}</Badge>
              {:else if mic}
                <Badge variant={mic.heard && !mic.late ? "success" : "warning"}>{mic.heard ? "Heard you" : "Nothing heard"}</Badge>
              {/if}
            </div>
            <div class="flex h-6 items-end gap-[3px]" aria-hidden="true">
              {#each Array(32) as _, i (i)}
                <span
                  class="flex-1 rounded-full transition-colors duration-75 {i / 32 < meter(level) ? 'bg-primary' : 'bg-accent dark:bg-input/50'}"
                  style:height="{30 + 70 * Math.sin((i / 31) * Math.PI) ** 0.6}%"
                ></span>
              {/each}
            </div>
          </div>
          {#if micError}
            <Alert variant="error" title="Couldn’t open the microphone">{micError}</Alert>
          {:else if mic && mic.late}
            <Alert variant="warning" title={mic.leading_ms === null ? "Your mic sent only silence" : `Your mic starts ${mic.leading_ms} ms late`}>
              {mic.leading_ms === null
                ? "Check that it isn't muted, and that Windows lets apps use the microphone."
                : "Windows Audio enhancements are muting the first moment of sound, so your first word gets clipped. Turn them off: Sound settings › your microphone › Audio enhancements › Off."}
              {#snippet actions()}
                <Button variant="outline" size="sm" onclick={() => api.openLink("ms-settings:sound")}><ExternalLink aria-hidden="true" />Sound settings</Button>
                <Button variant="ghost" size="sm" onclick={runMicCheck}><RefreshCw aria-hidden="true" />Check again</Button>
              {/snippet}
            </Alert>
          {:else if mic && !mic.heard}
            <Alert variant="warning" title="Bolo didn’t hear anything">
              {mic.fell_back
                ? "The microphone chosen in Settings isn’t plugged in, so Bolo listened on the Windows default."
                : "Is the right microphone the Windows default? You can also pick one in Settings › General."}
              {#snippet actions()}
                <Button variant="outline" size="sm" onclick={() => api.openLink("ms-settings:sound")}><ExternalLink aria-hidden="true" />Sound settings</Button>
                <Button variant="ghost" size="sm" onclick={runMicCheck}><RefreshCw aria-hidden="true" />Check again</Button>
              {/snippet}
            </Alert>
          {:else if mic}
            {@render ok(`Bolo hears you from the first moment (${mic.leading_ms} ms).`)}
          {/if}
        {:else if step === 2}
          {@render stepIcon(Cpu)}
          <div class="flex flex-col gap-2">
            <h1 class="text-2xl font-semibold tracking-[-0.025em]">Get the speech model</h1>
            <p class="text-[15px] leading-6 text-muted-foreground">
              {#if model}
                Picked for this PC ({Math.round((view?.ram_mb ?? 0) / 1024)} GB RAM, {model.hinglish ? "English and Hinglish" : "English"}).
                It downloads once, then works offline.
              {:else}
                This PC doesn't have the memory for an offline model. Use a cloud engine instead.
              {/if}
            </p>
          </div>
          {#if model}
            <div class="flex flex-col gap-3 rounded-xl border border-primary/40 bg-card px-4 py-3.5 shadow-xs/5 ring-3 ring-primary/8">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium">{model.name}</span>
                <Badge variant="primary">Recommended</Badge>
                <span class="flex-1"></span>
                <span class="text-xs text-muted-foreground tabular-nums">{megabytes(model.download_bytes)}</span>
              </div>
              {#if model.installed}
                {@render ok("Downloaded and ready.")}
              {:else if download && download.phase !== "failed"}
                <div class="progress" role="progressbar" aria-label="Download progress" aria-valuenow={percent(download)} aria-valuemin="0" aria-valuemax="100">
                  <div style:width="{download.phase === 'downloading' ? percent(download) : 100}%"></div>
                </div>
                <div class="flex items-center justify-between gap-4">
                  <span class="text-xs text-muted-foreground tabular-nums">
                    {download.phase === "downloading"
                      ? `${percent(download)}%${timeLeft(download)}`
                      : download.phase === "verifying"
                        ? "Checking the download…"
                        : "Unpacking…"}
                  </span>
                  <Button variant="ghost-muted" size="xs" onclick={() => api.cancelDownload(model.id)}><Pause aria-hidden="true" />Pause</Button>
                </div>
                <span class="-mt-1 text-2xs text-muted-foreground">Resumes if you close this window.</span>
              {:else}
                {#if modelError}<p class="flex items-center gap-1.5 text-xs text-destructive-foreground" role="alert"><CircleAlert class="size-3.5" aria-hidden="true" />{modelError}</p>{/if}
                <div><Button variant="primary" onclick={startDownload}><Download aria-hidden="true" />{modelError ? "Try again" : "Download"}</Button></div>
              {/if}
            </div>
            <div class="flex items-center gap-3 text-xs text-muted-foreground" aria-hidden="true">
              <span class="h-px flex-1 bg-border"></span>or<span class="h-px flex-1 bg-border"></span>
            </div>
          {/if}
          <div class="flex flex-col gap-2.5 rounded-xl border border-border/80 bg-card px-4 py-3.5 shadow-xs/5 dark:border-border">
            <div class="flex items-center gap-2">
              <Cloud class="size-4 text-muted-foreground" aria-hidden="true" />
              <span class="text-sm font-medium">Use a cloud engine</span>
            </div>
            <span class="text-xs text-muted-foreground">No download. Audio goes to Deepgram. Needs an API key, kept in Windows Credential Manager.</span>
            {#if cloudReady}
              {@render ok("Using Deepgram.")}
            {:else}
              <form
                class="flex gap-2"
                onsubmit={(e) => {
                  e.preventDefault();
                  useCloud();
                }}
              >
                <label for="cloud-key" class="sr-only">Deepgram API key</label>
                <input id="cloud-key" class="input mono text-xs!" type="password" autocomplete="off" spellcheck="false" placeholder="Deepgram API key…" bind:value={cloudKey} />
                <Button type="submit" variant="outline" disabled={!cloudKey.trim()}>Use Deepgram</Button>
              </form>
              {#if cloudError}<p class="flex items-center gap-1.5 text-xs text-destructive-foreground" role="alert"><CircleAlert class="size-3.5" aria-hidden="true" />{cloudError}</p>{/if}
            {/if}
          </div>
        {:else}
          {@render stepIcon(Keyboard)}
          <div class="flex flex-col gap-2">
            <h1 class="text-2xl font-semibold tracking-[-0.025em]">Try it</h1>
            <p class="text-[15px] leading-6 text-muted-foreground">
              Click in the box, hold <Kbd>{key}</Kbd> and say “Hello, this is my first dictation.”
            </p>
          </div>
          <label for="practice" class="sr-only">Practice box</label>
          <!-- svelte-ignore a11y_autofocus -->
          <textarea id="practice" class="textarea h-32 text-[15px]" placeholder="Your words land here…" autofocus bind:value={practice}></textarea>
          {#if latency !== null}
            <Alert variant="success" title="It works">Text in {Math.round(latency)} ms after you let go.</Alert>
          {:else if tryError}
            <Alert variant="error">{tryError}</Alert>
          {/if}
          <p class="text-xs text-muted-foreground">Bolo lives in the tray. Click its icon for settings and history; right-click to pause.</p>
        {/if}
      </div>
    {/key}
  </main>

  <footer class="flex items-center gap-2 border-t border-border px-6 py-3.5">
    {#if step > 0}<Button variant="ghost" onclick={() => go(step - 1)}><ArrowLeft aria-hidden="true" />Back</Button>{/if}
    <span class="flex-1"></span>
    {#if step === 0}
      <Button variant="primary" onclick={() => go(1)}>Get started<ArrowRight aria-hidden="true" /></Button>
    {:else if step === 1}
      <Button variant={!!mic && !mic.late && mic.heard ? "primary" : "outline"} disabled={checking} onclick={() => go(2)}>
        {mic && (mic.late || !mic.heard) ? "Continue anyway" : "Continue"}<ArrowRight aria-hidden="true" />
      </Button>
    {:else if step === 2}
      <Button variant="primary" disabled={!modelReady && !cloudReady} onclick={() => go(3)}>Next<ArrowRight aria-hidden="true" /></Button>
    {:else}
      <Button variant="primary" onclick={finish}>Finish<Check aria-hidden="true" /></Button>
    {/if}
  </footer>
</div>
