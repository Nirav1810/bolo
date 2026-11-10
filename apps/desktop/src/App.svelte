<script lang="ts">
  // Debug window (temporary, M3 ticket 01): renders the controller's events so a
  // dictation started from the tray can be watched. The designed settings, history and
  // onboarding screens replace it (tickets 04, 09, 10).
  import { invoke } from "@tauri-apps/api/core";
  import { onAppEvent, type AppEvent, type DictationState, type Output } from "./lib/events";

  type Result = { kind: "final" | "polished"; output: Output; at: Date };

  let dictation: DictationState = $state("Idle");
  let paused = $state(false);
  let level = $state(0);
  let partial = $state("");
  let results: Result[] = $state([]);
  let errors: string[] = $state([]);
  let route = $state(location.hash.slice(1) || "debug");

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
    const onHash = () => (route = location.hash.slice(1) || "debug");
    addEventListener("hashchange", onHash);
    invoke<{ state: DictationState; paused: boolean }>("status").then((s) => {
      dictation = s.state;
      paused = s.paused;
    });
    return () => {
      unlisten.then((f) => f());
      removeEventListener("hashchange", onHash);
    };
  });

  // RMS of speech sits around 0.01–0.2; show it on a log scale.
  const meter = $derived(Math.max(0, Math.min(1, (Math.log10(Math.max(level, 1e-4)) + 4) / 3.3)));
</script>

<main class="flex h-screen flex-col gap-4 bg-neutral-50 p-5 text-sm text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
  {#if route === "debug"}
    <header class="flex items-center justify-between">
      <h1 class="text-base font-semibold">Bolo debug</h1>
      <span class="rounded-full bg-neutral-200 px-3 py-1 text-xs dark:bg-neutral-800">
        {paused ? "Paused" : dictation}
      </span>
    </header>

    <section class="flex items-center gap-3">
      <button
        class="rounded-md bg-neutral-900 px-3 py-1.5 text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900"
        disabled={paused}
        onclick={() => invoke("toggle_dictation")}
      >
        {dictation === "Listening" ? "Stop" : "Start"} dictation
      </button>
      <div class="h-2 flex-1 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800" aria-label="Input level">
        <div class="h-full bg-emerald-500 transition-[width] duration-75" style:width="{meter * 100}%"></div>
      </div>
    </section>

    <p class="min-h-6 truncate text-neutral-500" dir="rtl" title={partial}>
      <bdi>{partial || (dictation === "Listening" ? "listening…" : "")}</bdi>
    </p>

    {#each errors as message}
      <p class="rounded-md bg-red-100 px-3 py-2 text-red-900 dark:bg-red-950 dark:text-red-200">{message}</p>
    {/each}

    <ol class="flex flex-1 flex-col gap-2 overflow-y-auto">
      {#each results as r}
        <li class="rounded-md border border-neutral-200 p-3 dark:border-neutral-800">
          <p class="whitespace-pre-wrap">{r.output.plain || "(nothing heard)"}</p>
          <p class="mt-1 text-xs text-neutral-500">
            {r.kind === "polished" ? `polished +${r.output.timings.llm_ms.toFixed(0)} ms` : `${r.output.timings.total_ms.toFixed(0)} ms`}
            · {r.output.engine_id} · {r.output.language} · {r.at.toLocaleTimeString()}
          </p>
        </li>
      {/each}
    </ol>
  {:else}
    <h1 class="text-base font-semibold">{route === "settings" ? "Settings" : "History"}</h1>
    <p class="text-neutral-500">Not built yet (M3 tickets 09 and 07). <a class="underline" href="#debug">Back to debug</a></p>
  {/if}
</main>
