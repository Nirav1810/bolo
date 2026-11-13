<script lang="ts">
  // The debug page (M3 ticket 01): renders the controller's events so a dictation can be
  // watched. Reached from the tray's "Debug window". Built from the same parts and column
  // as the settings pages.
  import { invoke } from "@tauri-apps/api/core";
  import { AudioLines } from "@lucide/svelte";
  import { LANGUAGE_NAMES, api, sentence, variant } from "./lib/api";
  import { onAppEvent, type AppEvent, type DictationState, type Output } from "./lib/events";
  import Alert from "./lib/ui/Alert.svelte";
  import AppMark from "./lib/ui/AppMark.svelte";
  import Badge from "./lib/ui/Badge.svelte";
  import Button from "./lib/ui/Button.svelte";
  import Empty from "./lib/ui/Empty.svelte";
  import LevelMeter from "./lib/ui/LevelMeter.svelte";
  import PageHeader from "./lib/ui/PageHeader.svelte";
  import SettingsRow from "./lib/ui/SettingsRow.svelte";
  import SettingsSection from "./lib/ui/SettingsSection.svelte";

  type Result = { kind: "final" | "polished"; output: Output; at: Date };

  let dictation: DictationState = $state("Idle");
  let paused = $state(false);
  let level = $state(0);
  let partial = $state("");
  let results: Result[] = $state([]);
  let errors: string[] = $state([]);
  let version = $state<string | null>(null);

  function apply(e: AppEvent) {
    switch (e.kind) {
      case "state":
        dictation = e.state;
        if (e.state === "Listening") partial = "";
        break;
      case "paused":
        paused = e.paused;
        break;
      case "level":
        level = e.rms;
        break;
      case "partial":
        partial = e.text;
        break;
      case "final":
      case "polished":
        results = [{ kind: e.kind, output: e.output, at: new Date() }, ...results].slice(0, 20);
        break;
      case "error":
        errors = [e.message, ...errors].slice(0, 5);
        break;
    }
  }

  $effect(() => {
    const unlisten = onAppEvent(apply);
    invoke<{ state: DictationState; paused: boolean }>("status").then((s) => {
      dictation = s.state;
      paused = s.paused;
    });
    // The build names the page, so Bolo Nightly's is never taken for Bolo's.
    api
      .updateView()
      .then((v) => (version = v.version))
      .catch(() => {});
    return () => {
      unlisten.then((f) => f());
    };
  });

  const product = $derived(version === null ? "Bolo" : variant(version).product);
  // The pill's words for the same states.
  const STATE: Record<DictationState, { text: string; variant: "neutral" | "primary" | "info" | "success" }> = {
    Idle: { text: "Idle", variant: "neutral" },
    Listening: { text: "Listening…", variant: "primary" },
    Processing: { text: "Transcribing…", variant: "info" },
    Done: { text: "Done", variant: "success" },
  };
  const badge = $derived(paused ? { text: "Paused", variant: "neutral" as const } : STATE[dictation]);
</script>

<main class="h-screen overflow-y-auto bg-background [scrollbar-gutter:stable]">
  <div class="enter mx-auto flex w-full max-w-3xl flex-col gap-8 px-8 pt-9 pb-16">
    <div class="px-1"><AppMark {version} /></div>
    <PageHeader title="{product} debug" blurb="The controller’s events as they arrive, so a dictation can be watched." />

    {#each errors as message, i (i)}
      <Alert variant="error" title="Couldn’t dictate">{sentence(message)}</Alert>
    {/each}

    <SettingsSection title="Dictation">
      <SettingsRow title="State">
        {#snippet badges()}<Badge variant={badge.variant}>{badge.text}</Badge>{/snippet}
        {#snippet description()}
          <!-- Right to left, so a long partial keeps its newest words in view. -->
          <p class="truncate text-left" dir="rtl">
            <bdi>{partial || (dictation === "Listening" ? "Listening…" : "Hold your push-to-talk key, or start a dictation here.")}</bdi>
          </p>
        {/snippet}
        {#snippet control()}
          <Button variant="primary" size="sm" disabled={paused} onclick={() => invoke("toggle_dictation")}>
            {dictation === "Listening" ? "Stop" : "Start"} dictation
          </Button>
        {/snippet}
        <LevelMeter rms={level} label="Input level" />
      </SettingsRow>
    </SettingsSection>

    <SettingsSection title="Results" as="ul">
      {#each results as r, i (i)}
        <SettingsRow as="li" align="start">
          {#snippet title()}<span class="whitespace-pre-wrap">{r.output.plain || "Nothing heard"}</span>{/snippet}
          {#snippet badges()}
            <Badge variant={r.kind === "polished" ? "primary" : "neutral"}>{r.kind === "polished" ? "Polished" : "Final"}</Badge>
          {/snippet}
          {#snippet description()}
            <span class="tabular-nums">
              {r.kind === "polished" ? `+${Math.round(r.output.timings.llm_ms)} ms` : `${Math.round(r.output.timings.total_ms)} ms`}
              · <span class="font-mono">{r.output.engine_id}</span> · {LANGUAGE_NAMES[r.output.language] ?? r.output.language} · {r.at.toLocaleTimeString()}
            </span>
          {/snippet}
        </SettingsRow>
      {:else}
        <li><Empty icon={AudioLines} title="No results yet" hint="Each dictation’s text and timings show here as it finishes." /></li>
      {/each}
    </SettingsSection>
  </div>
</main>
