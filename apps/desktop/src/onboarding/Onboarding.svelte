<script lang="ts">
  // First-run onboarding (M3 ticket 10, design approved in ticket 04): welcome → microphone
  // check → model → try it. Each step is saved, so quitting partway resumes there.
  // One frame for every step: the header and footer stay put and each step's content
  // starts at the same place, in the settings column, so nothing jumps between steps.
  import { tick } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ArrowLeft, ArrowRight, Cloud, Cpu, Download, ExternalLink, Keyboard, Mic, RefreshCw, TextCursorInput } from "@lucide/svelte";
  import Alert from "../lib/ui/Alert.svelte";
  import AppMark from "../lib/ui/AppMark.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Card from "../lib/ui/Card.svelte";
  import IconTile from "../lib/ui/IconTile.svelte";
  import KeyField from "../lib/ui/KeyField.svelte";
  import Kbd from "../lib/ui/Kbd.svelte";
  import LevelMeter from "../lib/ui/LevelMeter.svelte";
  import PageHeader from "../lib/ui/PageHeader.svelte";
  import Progress from "../lib/ui/Progress.svelte";
  import Select from "../lib/ui/Select.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Skeleton from "../lib/ui/Skeleton.svelte";
  import {
    api,
    downloadCaption,
    downloadPercent,
    megabytes,
    onDownload,
    onMicLevel,
    saveCheckedKey,
    sentence,
    type DownloadEvent,
    type MicCheck,
    type Microphones,
    type SettingsView,
  } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  import { PAGES } from "../lib/pages";
  import { loudness } from "../lib/waveform";

  let { ondone }: { ondone: () => void } = $props();

  type Problem = { title: string; detail: string };
  type RowStatus = { tone: "success" | "warning" | "error" | "busy"; text: string };

  const STEPS = 4;
  let step = $state(0);
  let view = $state<SettingsView | null>(null);
  let loadError = $state<Problem | null>(null);
  let column: HTMLElement | undefined = $state();

  // Microphone check.
  let checking = $state(false);
  let level = $state(0);
  let mic = $state<MicCheck | null>(null);
  let micError = $state<Problem | null>(null);
  // The microphones plugged in now, read again each time the list opens (hot-plug).
  let mics = $state<Microphones | null>(null);

  // Model.
  let download = $state<DownloadEvent | null>(null);
  let started = 0;
  let modelError = $state<Problem | null>(null);
  let cloudNote = $state<string | null>(null);

  // Try it.
  let practice = $state("");
  let latency = $state<number | null>(null);
  let tryError = $state("");
  let finishing = $state(false);

  const model = $derived(view?.models.find((m) => m.recommended) ?? null);
  const primary = $derived(view?.form.engines[0] ?? "");
  const cloudReady = $derived(!!view && !primary.startsWith("local:") && !!view.keys.deepgram);
  const modelReady = $derived(!!model?.installed && primary === model.spec);
  const key = $derived(view?.key_name ?? "Caps Lock");
  const ready = $derived(view !== null || loadError !== null);

  const chosenMic = $derived(view?.form.input_device ?? null);
  const micMissing = $derived(mics !== null && chosenMic !== null && !mics.devices.includes(chosenMic));
  const micOptions = $derived<[string, string][]>([
    ["", "Windows default"],
    ...(mics?.devices ?? []).map((name): [string, string] => [name, name]),
    ...(chosenMic !== null && (mics === null || micMissing) ? [[chosenMic, micMissing ? `${chosenMic} (not plugged in)` : chosenMic] as [string, string]] : []),
  ]);
  const settingsGeneral = `Settings › ${PAGES.general.label}`;

  async function load() {
    // The webview loads with the app, before the app's state is ready: try a few times.
    for (let attempt = 0; ; attempt++) {
      try {
        view = await api.settings();
        loadError = null;
        return;
      } catch (e) {
        if (attempt >= 20) {
          loadError = { title: "Couldn’t load settings", detail: sentence(String(e)) };
          return;
        }
        await new Promise((r) => setTimeout(r, 250));
      }
    }
  }

  async function refreshMics() {
    try {
      mics = await api.microphones();
    } catch {
      // Keep the last list; the select still shows the choice.
    }
  }

  $effect(() => {
    load().then(() => {
      step = Math.min(view?.onboarding_step ?? 0, STEPS - 1);
      if (view?.downloading.length && model) download = { id: model.id, phase: "downloading", downloaded: 0, total: 0, error: null };
      if (step === 1) runMicCheck();
    });
    refreshMics();
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
    // The pressed button may be gone; put focus on the new step (its field, or its title).
    await tick();
    const target = column?.querySelector<HTMLElement>("[autofocus]") ?? column?.querySelector<HTMLElement>("h1");
    if (target?.tagName === "H1") target.tabIndex = -1;
    target?.focus({ preventScroll: true });
    await api.onboardingSave(to);
    if (to === 1 && !mic) runMicCheck();
  }

  async function runMicCheck() {
    checking = true;
    micError = null;
    mic = null;
    try {
      mic = await api.micCheck();
    } catch (e) {
      micError = { title: "Couldn’t open the microphone", detail: sentence(String(e)) };
    }
    checking = false;
    level = 0;
  }

  /** Saves the microphone picked here, as General does, and checks it again. */
  async function chooseMic(value: string) {
    if (!view) return;
    const form = $state.snapshot(view.form);
    form.input_device = value === "" ? null : value;
    view.form = form;
    try {
      view = await api.save(form);
    } catch (e) {
      micError = { title: "Couldn’t save the microphone", detail: sentence(String(e)) };
      return;
    }
    runMicCheck();
  }

  async function onDownloadEvent(e: DownloadEvent) {
    if (e.id !== model?.id) return;
    download = e;
    if (e.phase === "failed") modelError = { title: `Couldn’t download ${model.name}`, detail: sentence(e.error ?? "The download failed.") };
    if (e.phase === "done" || e.phase === "cancelled" || e.phase === "failed") {
      if (e.phase !== "failed") download = null;
      view = await api.settings();
      if (e.phase === "done") await useModel();
    }
  }

  async function startDownload() {
    if (!model) return;
    modelError = null;
    started = Date.now();
    download = { id: model.id, phase: "downloading", downloaded: 0, total: model.download_bytes, error: null };
    try {
      await api.download(model.id);
    } catch (e) {
      download = null;
      modelError = { title: `Couldn’t download ${model.name}`, detail: sentence(String(e)) };
    }
  }

  /** Put the downloaded model first in the engine chain. */
  async function useModel() {
    if (!view || !model) return;
    const engines = [model.spec, ...view.form.engines.filter((e) => e !== model.spec)];
    view = await api.save({ ...$state.snapshot(view.form), engines });
  }

  /** Checks and saves the Deepgram API key and makes Deepgram the primary engine; resolves to what went wrong, for the KeyField. */
  async function useCloud(pasted: string): Promise<string | null> {
    if (!view) return null;
    const r = await saveCheckedKey("deepgram", pasted);
    if (!r.saved) return r.error;
    view.keys.deepgram = r.hint;
    try {
      const engines = ["deepgram-stream", ...view.form.engines.filter((e) => e !== "deepgram-stream")];
      view = await api.save({ ...$state.snapshot(view.form), engines });
    } catch (e) {
      return `Couldn’t use Deepgram: ${sentence(String(e))}`;
    }
    cloudNote = r.note;
    return null;
  }

  /** " · about 2 min left", once the download has run long enough to tell. */
  function timeLeft(d: DownloadEvent): string {
    const secs = (Date.now() - started) / 1000;
    if (d.phase !== "downloading" || !started || secs < 2 || !d.downloaded || !d.total) return "";
    const left = (d.total - d.downloaded) / (d.downloaded / secs);
    return left < 60 ? ` · about ${Math.max(1, Math.round(left))} s left` : ` · about ${Math.round(left / 60)} min left`;
  }

  async function finish() {
    finishing = true;
    await api.onboardingSave(null);
    // Closing destroys the window and its webview; the tray builds it again.
    await getCurrentWindow().close();
    ondone();
  }

  const forward = $derived(step === 0 ? "Get started" : step === STEPS - 1 ? "Finish" : "Continue");
  const blocked = $derived(!ready || (step === 1 && checking) || (step === 2 && !modelReady && !cloudReady));

  const micStatus = $derived.by((): RowStatus | null => {
    if (checking) return { tone: "busy", text: loudness(level) > 0 ? "Hearing you…" : "Listening…" };
    if (mic && mic.heard && !mic.late)
      return { tone: "success", text: mic.leading_ms === null ? "Bolo hears you." : `Bolo hears you from the first moment (${Math.round(mic.leading_ms)} ms).` };
    return null;
  });
  const modelStatus = $derived.by((): RowStatus | null => {
    if (!model?.installed) return null;
    return modelReady ? { tone: "success", text: "Downloaded and ready." } : { tone: "success", text: "Downloaded." };
  });
</script>

<div class="flex h-screen flex-col bg-background">
  <header class="flex-none">
    <div class="mx-auto flex w-full max-w-3xl items-center justify-between gap-4 px-8 pt-5">
      <AppMark version={view?.version ?? null} />
      {#if ready}
        <div class="flex items-center gap-3">
          <p class="text-xs text-muted-foreground tabular-nums">Step {step + 1} of {STEPS}</p>
          <div class="flex gap-1" aria-hidden="true">
            {#each Array(STEPS) as _, i (i)}
              <span class="h-1 w-6 rounded-full transition-colors {i <= step ? 'bg-primary' : 'bg-border'}"></span>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </header>

  <main class="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
    {#key step}
      <div bind:this={column} class="enter mx-auto flex w-full max-w-3xl flex-col gap-8 px-8 pt-9 pb-16 [&_h1]:outline-none">
        {#if !ready}
          <Skeleton />
        {:else}
          {#if step === 0}
            <PageHeader
              size="hero"
              title="Talk in any app. Bolo types it."
              blurb="English, Hinglish or both in one sentence. It runs on this PC, so your voice stays here unless you pick a cloud engine."
            />
          {:else if step === 1}
            <PageHeader size="hero" title="Check your microphone" blurb="Say anything for three seconds. Bolo checks that it hears the very start of what you say." />
          {:else if step === 2}
            <PageHeader
              size="hero"
              title="Get the speech model"
              blurb={model
                ? `Picked for this PC (${Math.round((view?.ram_mb ?? 0) / 1024)} GB RAM, ${model.hinglish ? "English and Hinglish" : "English"}). It downloads once, then works offline.`
                : "This PC doesn’t have the memory for an offline model. Use a cloud engine instead."}
            />
          {:else}
            <PageHeader size="hero" title="Try it">
              {#snippet blurb()}Click in the box, hold <Kbd>{key}</Kbd> and say “Hello, this is my first dictation.”{/snippet}
            </PageHeader>
          {/if}

          {#if loadError}<Alert variant="error" title={loadError.title}>{loadError.detail}</Alert>{/if}

          {#if step === 0}
            <Card as="ol" divided aria-label="How it works">
              <SettingsRow as="li" description={key === "Caps Lock" ? "A quick tap of Caps Lock still types capitals." : undefined}>
                {#snippet leading()}<IconTile icon={Keyboard} />{/snippet}
                {#snippet title()}Hold <Kbd>{key}</Kbd> anywhere you can type{/snippet}
              </SettingsRow>
              <SettingsRow as="li" title="Speak naturally, in English or Hinglish">
                {#snippet leading()}<IconTile icon={Mic} />{/snippet}
              </SettingsRow>
              <SettingsRow as="li" title="Let go, and the text is inserted where your cursor is">
                {#snippet leading()}<IconTile icon={TextCursorInput} />{/snippet}
              </SettingsRow>
            </Card>
          {:else if step === 1}
            <SettingsSection>
              <SettingsRow title="Microphone" labelFor="microphone" status={micStatus}>
                {#snippet leading()}
                  <IconTile icon={Mic} tone={micError ? "error" : mic && (mic.late || !mic.heard) ? "warning" : mic ? "success" : "neutral"} />
                {/snippet}
                {#snippet description()}
                  {#if micMissing}
                    Not plugged in. Bolo uses the Windows default until it’s back.
                  {:else}
                    You can change it later in {settingsGeneral}.
                  {/if}
                {/snippet}
                {#snippet control()}
                  <Select
                    id="microphone"
                    class="w-56"
                    value={chosenMic ?? ""}
                    options={micOptions}
                    disabled={!view || checking}
                    onopen={refreshMics}
                    onchange={chooseMic}
                  />
                {/snippet}
                <LevelMeter rms={level} />
              </SettingsRow>
              {#snippet footer()}
                {#if micError}
                  <Alert variant="error" title={micError.title}>
                    {micError.detail}
                    {#snippet actions()}
                      <Button variant="outline" size="sm" onclick={runMicCheck}><RefreshCw aria-hidden="true" />Try again</Button>
                    {/snippet}
                  </Alert>
                {:else if mic && (mic.late || !mic.heard)}
                  <Alert
                    variant="warning"
                    live
                    title={!mic.heard
                      ? "Bolo didn’t hear anything"
                      : mic.leading_ms === null
                        ? "Your microphone sent only silence"
                        : `Your microphone starts ${Math.round(mic.leading_ms)} ms late`}
                  >
                    {#if !mic.heard}
                      {mic.fell_back
                        ? "The microphone you chose isn’t plugged in, so Bolo listened on the Windows default. Pick another above."
                        : "Is the right microphone chosen above? Check that it isn’t muted."}
                    {:else if mic.leading_ms === null}
                      Check that it isn’t muted, and that Windows lets apps use the microphone.
                    {:else}
                      Windows Audio enhancements are muting the first moment of sound, so your first word gets clipped. Turn them off: Sound
                      settings › your microphone › Audio enhancements › Off.
                    {/if}
                    {#snippet actions()}
                      <Button variant="outline" size="sm" onclick={runMicCheck}><RefreshCw aria-hidden="true" />Check</Button>
                      <Button variant="ghost" size="sm" onclick={() => api.openLink("ms-settings:sound")}>Sound settings<ExternalLink aria-hidden="true" /></Button>
                    {/snippet}
                  </Alert>
                {/if}
              {/snippet}
            </SettingsSection>
          {:else if step === 2}
            {#if model}
              <SettingsSection title="On this PC" description="Private and offline. Your audio never leaves this PC.">
                <SettingsRow title={model.name} status={modelStatus}>
                  {#snippet leading()}<IconTile icon={Cpu} tone={modelReady ? "primary" : "neutral"} />{/snippet}
                  {#snippet badges()}<Badge variant="primary">Recommended</Badge>{/snippet}
                  {#snippet description()}
                    {model.hinglish ? "English and Hinglish" : "English only"} · <span class="tabular-nums">{megabytes(model.download_bytes)}</span>
                  {/snippet}
                  {#snippet control()}
                    {#if download && download.phase !== "failed"}
                      <Button variant="ghost" size="sm" onclick={() => api.cancelDownload(model.id)}>Cancel</Button>
                    {:else if !model.installed}
                      <Button variant="outline" size="sm" onclick={startDownload}><Download aria-hidden="true" />{modelError ? "Try again" : "Download"}</Button>
                    {:else if !modelReady}
                      <Button variant="outline" size="sm" onclick={useModel}>Make primary</Button>
                    {/if}
                  {/snippet}
                  {#if download && download.phase !== "failed" && !model.installed}
                    <Progress
                      label="Downloading {model.name}"
                      value={downloadPercent(download)}
                      caption="{downloadCaption(download, model.download_bytes)}{timeLeft(download)} · Resumes if you close this window."
                    />
                  {/if}
                </SettingsRow>
                {#snippet footer()}
                  {#if modelError}<Alert variant="error" title={modelError.title}>{modelError.detail}</Alert>{/if}
                {/snippet}
              </SettingsSection>
            {/if}
            <SettingsSection title="Cloud" description="No download. Your audio is sent to Deepgram.">
              <SettingsRow
                title="Deepgram"
                labelFor={cloudReady ? undefined : "deepgram-key"}
                status={cloudReady ? (cloudNote ? { tone: "warning", text: cloudNote } : { tone: "success", text: "Using Deepgram." }) : null}
                description="Needs an API key, kept in Windows Credential Manager."
              >
                {#snippet leading()}<IconTile icon={Cloud} tone={cloudReady ? "primary" : "neutral"} />{/snippet}
                {#if !cloudReady}
                  <KeyField
                    id="deepgram-key"
                    size="default"
                    label="Deepgram API key"
                    placeholder="Paste Deepgram API key…"
                    submit="Use Deepgram"
                    disabled={!view?.keystore}
                    onsave={useCloud}
                  />
                {/if}
              </SettingsRow>
              {#snippet footer()}
                {#if view && !view.keystore}
                  <Alert variant="warning" title="API keys can’t be saved">Windows Credential Manager couldn’t be opened.</Alert>
                {/if}
              {/snippet}
            </SettingsSection>
          {:else}
            <div class="flex flex-col gap-2.5">
              <label for="practice" class="sr-only">Practice box</label>
              <!-- svelte-ignore a11y_autofocus -->
              <textarea id="practice" class="field h-32" placeholder="Your words are inserted here…" autofocus bind:value={practice}></textarea>
              {#if latency !== null}
                <Alert variant="success" live title="It works">Text inserted {Math.round(latency)} ms after you let go.</Alert>
              {:else if tryError}
                <Alert variant="error" title="Couldn’t dictate">{sentence(tryError)}</Alert>
              {/if}
            </div>
            <Alert title="Bolo lives in the tray">Click its icon for settings and history. Right-click it to pause.</Alert>
          {/if}
        {/if}
      </div>
    {/key}
  </main>

  <footer class="flex-none border-t border-border">
    <div class="mx-auto flex w-full max-w-3xl items-center gap-2 px-8 py-3.5">
      {#if step > 0}<Button variant="ghost" onclick={() => go(step - 1)}><ArrowLeft aria-hidden="true" />Back</Button>{/if}
      <span class="flex-1"></span>
      {#if step === 2 && ready && !modelReady && !cloudReady}
        <p class="pr-2 text-xs text-muted-foreground">Download the model or add a Deepgram API key to continue.</p>
      {/if}
      <Button variant="primary" disabled={blocked} loading={finishing} onclick={() => (step === STEPS - 1 ? finish() : go(step + 1))}>
        {forward}<ArrowRight aria-hidden="true" />
      </Button>
    </div>
  </footer>
</div>
