<script lang="ts">
  // The debug page (M3 ticket 01): renders the controller's events so a dictation can be
  // watched. Reached from the tray's "Debug window".
  import { invoke } from "@tauri-apps/api/core";
  import { onAppEvent, type AppEvent, type DictationState, type Output } from "./lib/events";

  type Result = { kind: "final" | "polished"; output: Output; at: Date };

  let dictation: DictationState = $state("Idle");
  let paused = $state(false);
  let level = $state(0);
  let partial = $state("");
  let results: Result[] = $state([]);
  let errors: string[] = $state([]);

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
    return () => {
      unlisten.then((f) => f());
    };
  });

  // RMS of speech sits around 0.01–0.2; show it on a log scale.
  const meter = $derived(Math.max(0, Math.min(1, (Math.log10(Math.max(level, 1e-4)) + 4) / 3.3)));
</script>

<main class="flex h-screen flex-col gap-4 bg-background p-5 text-sm text-foreground">
    <header class="flex items-center justify-between">
      <h1 class="text-base font-semibold">Bolo debug</h1>
      <span class="rounded-md bg-accent px-2 py-0.5 text-xs font-medium text-muted-foreground">
        {paused ? "Paused" : dictation}
      </span>
    </header>

    <section class="flex items-center gap-3">
      <button
        class="h-8 rounded-lg border border-primary bg-primary px-3 font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        disabled={paused}
        onclick={() => invoke("toggle_dictation")}
      >
        {dictation === "Listening" ? "Stop" : "Start"} dictation
      </button>
      <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-accent" aria-label="Input level">
        <div class="h-full bg-success transition-[width] duration-75" style:width="{meter * 100}%"></div>
      </div>
    </section>

    <p class="min-h-6 truncate text-muted-foreground" dir="rtl" title={partial}>
      <bdi>{partial || (dictation === "Listening" ? "listening…" : "")}</bdi>
    </p>

    {#each errors as message}
      <p class="rounded-xl border border-destructive/30 bg-destructive/5 px-3.5 py-2.5 text-destructive-foreground">{message}</p>
    {/each}

    <ol class="flex flex-1 flex-col gap-2 overflow-y-auto">
      {#each results as r}
        <li class="rounded-xl border border-border bg-card p-3">
          <p class="whitespace-pre-wrap">{r.output.plain || "(nothing heard)"}</p>
          <p class="mt-1 text-xs text-muted-foreground">
            {r.kind === "polished" ? `polished +${r.output.timings.llm_ms.toFixed(0)} ms` : `${r.output.timings.total_ms.toFixed(0)} ms`}
            · {r.output.engine_id} · {r.output.language} · {r.at.toLocaleTimeString()}
          </p>
        </li>
      {/each}
    </ol>
</main>
