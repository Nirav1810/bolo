<script lang="ts">
  // The settings window (M3 ticket 09, design approved in ticket 04): a Windows 11
  // Settings layout, left nav and setting cards. Every change is saved and applied at
  // once by Rust (src-tauri/src/commands.rs); there is no Save button.
  import { api, type Form, type SettingsView } from "../lib/api";
  import General from "./General.svelte";
  import Engines from "./Engines.svelte";
  import Polish from "./Polish.svelte";
  import Dictionary from "./Dictionary.svelte";
  import History from "./History.svelte";
  import About from "./About.svelte";

  let { tab = $bindable("general") }: { tab?: string } = $props();

  const TABS = [
    ["general", "General"],
    ["engines", "Engines and keys"],
    ["polish", "Polish"],
    ["dictionary", "Dictionary"],
    ["history", "History"],
    ["about", "About"],
  ] as const;

  let view: SettingsView | null = $state(null);
  let error = $state("");

  async function load() {
    // The webview loads with the app, before the app's state is ready: try a few times.
    for (let attempt = 0; ; attempt++) {
      try {
        view = await api.settings();
        error = "";
        return;
      } catch (e) {
        if (attempt >= 20) {
          error = String(e);
          return;
        }
        await new Promise((r) => setTimeout(r, 250));
      }
    }
  }

  /** Change the form and save it; the view comes back as Rust saved it. */
  async function update(change: (f: Form) => void) {
    if (!view) return;
    const form: Form = $state.snapshot(view.form);
    change(form);
    view.form = form;
    try {
      view = await api.save(form);
      error = "";
    } catch (e) {
      error = `Couldn't save: ${e}`;
      await load();
    }
  }

  $effect(() => {
    load();
    // The window hides rather than closes, so refresh whenever it comes back.
    const onFocus = () => load();
    addEventListener("focus", onFocus);
    return () => removeEventListener("focus", onFocus);
  });

  const title = $derived(TABS.find(([id]) => id === tab)?.[1] ?? "Settings");
</script>

<div class="flex h-screen">
  <nav class="flex w-56 flex-none flex-col gap-0.5 px-2 py-3" aria-label="Settings sections">
    {#each TABS as [id, label]}
      <button
        type="button"
        class="nav relative flex h-9 w-full items-center rounded-[4px] px-3 text-left transition-colors duration-[120ms] hover:bg-hover"
        class:bg-subtle={tab === id}
        aria-current={tab === id ? "page" : undefined}
        onclick={() => (tab = id)}
      >
        {#if tab === id}<span class="absolute top-2.5 bottom-2.5 left-0 w-[3px] rounded-sm bg-accent"></span>{/if}
        {label}
      </button>
    {/each}
  </nav>

  <main
    class="flex min-w-0 flex-1 flex-col gap-1.5 overflow-y-auto rounded-tl-[8px] border-t border-l border-border bg-surface px-8 py-6"
  >
    <h1 class="mb-3 font-display text-title">{title}</h1>
    {#if error}
      <p class="note danger mb-2" role="alert"><span class="icon" aria-hidden="true">⚠</span>{error}</p>
    {/if}
    {#if view}
      {#if tab === "general"}
        <General {view} {update} />
      {:else if tab === "engines"}
        <Engines bind:view {update} />
      {:else if tab === "polish"}
        <Polish {view} {update} onkeys={() => (tab = "engines")} />
      {:else if tab === "dictionary"}
        <Dictionary {view} {update} />
      {:else if tab === "history"}
        <History {view} {update} />
      {:else}
        <About {view} />
      {/if}
    {:else if !error}
      <p class="caption">Loading…</p>
    {/if}
  </main>
</div>
