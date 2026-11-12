<script lang="ts">
  // The settings window: a T3 Code-style sidebar (icon nav in titled groups, the build
  // in the footer) and one centred column of setting sections. Every change is saved
  // and applied at once by Rust (src-tauri/src/commands.rs); there is no Save button.
  import {
    AppWindow,
    AudioLines,
    BookA,
    History as HistoryIcon,
    Info,
    SlidersHorizontal,
    Sparkles,
    ArrowUpCircle,
    CircleAlert,
    Gauge,
  } from "@lucide/svelte";
  import { api, variant, type Form, type SettingsView, type UpdateView } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  import Badge from "../lib/ui/Badge.svelte";
  import General from "./General.svelte";
  import Engines from "./Engines.svelte";
  import Polish from "./Polish.svelte";
  import Apps from "./Apps.svelte";
  import Dictionary from "./Dictionary.svelte";
  import Snippets from "./Snippets.svelte";
  import History from "./History.svelte";
  import About from "./About.svelte";
  import Usage from "./Usage.svelte";
  import releaseIcon from "../../src-tauri/icons/128x128.png?no-inline";
  import nightlyIcon from "../../src-tauri/icons-nightly/128x128.png?no-inline";

  let { tab = $bindable("general") }: { tab?: string } = $props();

  const PAGES = {
    general: { label: "General", icon: SlidersHorizontal, blurb: "How you start a dictation, and the language Bolo listens for." },
    engines: { label: "Engines and keys", icon: AudioLines, blurb: "The speech engines that turn your voice into text, and keys for the cloud ones." },
    polish: { label: "Polish", icon: Sparkles, blurb: "Claude tidies each dictation after it lands, then swaps the result in." },
    apps: { label: "Apps", icon: AppWindow, blurb: "How your words go into each app: the format, and polish and its tone." },
    dictionary: { label: "Dictionary & snippets", icon: BookA, blurb: "Names and terms Bolo should spell your way, and phrases it expands to saved text." },
    history: { label: "History", icon: HistoryIcon, blurb: "Your dictations, kept as text on this PC." },
    usage: { label: "Usage", icon: Gauge, blurb: "What your cloud engines used, and what it cost." },
    about: { label: "About", icon: Info, blurb: "Version, updates and where Bolo keeps its files." },
  } as const;
  type Page = keyof typeof PAGES;

  const GROUPS: { heading: string; pages: Page[] }[] = [
    { heading: "Dictation", pages: ["general", "engines", "polish", "apps", "dictionary"] },
    { heading: "Activity", pages: ["history", "usage"] },
    { heading: "App", pages: ["about"] },
  ];

  let view = $state<SettingsView | null>(null);
  let error = $state("");
  let updates = $state<UpdateView | null>(null);

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
    api
      .updateView()
      .then((v) => (updates = v))
      .catch(() => {});
    const off = onAppEvent((e) => {
      if (e.kind === "update" && updates) updates = { ...updates, status: e.status };
    });
    // Closing the window destroys it, so everything above loads afresh on each open.
    // Refresh on focus too: the tray or another app may have changed things meanwhile.
    const onFocus = () => load();
    addEventListener("focus", onFocus);
    return () => {
      removeEventListener("focus", onFocus);
      off.then((f) => f());
    };
  });

  const page = $derived((tab in PAGES ? tab : "general") as Page);
  const build = $derived(view ? variant(view.version) : null);
  const available = $derived(updates?.status.state === "available" ? updates.status.version : null);

  /** Up and Down move through the nav, as in a list. */
  function onNavKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const order = GROUPS.flatMap((g) => g.pages);
    const i = order.indexOf(page);
    const next = order[(i + (e.key === "ArrowDown" ? 1 : order.length - 1)) % order.length];
    tab = next;
    document.querySelector<HTMLElement>(`[data-page="${next}"]`)?.focus();
  }
</script>

<div class="flex h-screen overflow-hidden bg-background">
  <aside class="flex w-[228px] flex-none flex-col border-r border-sidebar-border bg-sidebar">
    <div class="flex items-center gap-2.5 px-4 pt-4 pb-3">
      <img src={build?.nightly ? nightlyIcon : releaseIcon} width="24" height="24" alt="" class="rounded-md" />
      <span class="text-[15px] font-semibold tracking-[-0.01em] text-sidebar-foreground">
        {updates?.product ?? (build?.nightly ? "Bolo Nightly" : "Bolo")}
      </span>
      {#if build}
        <Badge variant={build.nightly ? "nightly" : "secondary"}>{build.name}</Badge>
      {/if}
    </div>

    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <nav class="flex flex-1 flex-col gap-4 overflow-y-auto px-2 pt-2" aria-label="Settings sections" onkeydown={onNavKey}>
      {#each GROUPS as group (group.heading)}
        <div class="flex flex-col gap-px">
          <p class="px-2.5 pb-1 text-xs font-medium text-sidebar-muted-foreground/80">{group.heading}</p>
          {#each group.pages as id (id)}
            {@const p = PAGES[id]}
            {@const active = page === id}
            <button
              type="button"
              data-page={id}
              class="flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-sm outline-none transition-colors duration-100 focus-visible:ring-2 focus-visible:ring-ring {active
                ? 'bg-sidebar-row-selected font-medium text-sidebar-foreground shadow-xs/5 ring-1 ring-sidebar-border dark:shadow-none dark:ring-0'
                : 'font-medium text-sidebar-muted-foreground hover:bg-sidebar-row-hover hover:text-sidebar-foreground'}"
              aria-current={active ? "page" : undefined}
              onclick={() => (tab = id)}
            >
              <p.icon class="size-4 shrink-0 {active ? 'text-sidebar-foreground' : 'text-sidebar-muted-foreground/85'}" aria-hidden="true" />
              <span class="truncate">{p.label}</span>
            </button>
          {/each}
        </div>
      {/each}
    </nav>

    <div class="p-2">
      {#if available}
        <button
          type="button"
          class="flex w-full items-center gap-2.5 rounded-lg border border-primary/25 bg-primary/8 px-2.5 py-2 text-left outline-none transition-colors hover:bg-primary/12 focus-visible:ring-2 focus-visible:ring-ring"
          onclick={() => (tab = "about")}
        >
          <ArrowUpCircle class="size-4 shrink-0 text-primary-text" aria-hidden="true" />
          <span class="flex min-w-0 flex-col">
            <span class="text-[13px] font-medium text-foreground">Update available</span>
            <span class="truncate text-2xs text-muted-foreground">Version {available} is ready</span>
          </span>
        </button>
      {:else if view}
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs text-sidebar-muted-foreground outline-none transition-colors hover:bg-sidebar-row-hover hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring"
          title="Open About for updates"
          onclick={() => (tab = "about")}
        >
          <span class="size-1.5 shrink-0 rounded-full {build?.nightly ? 'bg-nightly' : 'bg-success'}" aria-hidden="true"></span>
          <span class="truncate">Version {view.version}</span>
        </button>
      {/if}
    </div>
  </aside>

  <main class="min-w-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
    {#key page}
      <div class="enter mx-auto flex w-full max-w-[700px] flex-col gap-8 px-8 pt-9 pb-16">
        <header class="flex flex-col gap-1 px-1">
          <h1 class="text-xl font-semibold tracking-[-0.02em] text-foreground">{PAGES[page].label}</h1>
          <p class="text-sm text-muted-foreground">{PAGES[page].blurb}</p>
        </header>

        {#if error}
          <div class="flex items-start gap-2.5 rounded-xl border border-destructive/30 bg-destructive/5 px-3.5 py-3 text-[13px]" role="alert">
            <CircleAlert class="mt-0.5 size-4 shrink-0 text-destructive-foreground" aria-hidden="true" />
            <span class="text-foreground">{error}</span>
          </div>
        {/if}

        {#if view}
          {#if page === "general"}
            <General {view} {update} />
          {:else if page === "engines"}
            <Engines bind:view {update} />
          {:else if page === "polish"}
            <Polish {view} {update} onkeys={() => (tab = "engines")} onapps={() => (tab = "apps")} />
          {:else if page === "apps"}
            <Apps bind:view onkeys={() => (tab = "engines")} />
          {:else if page === "dictionary"}
            <Dictionary {view} {update} />
            <Snippets {view} {update} />
          {:else if page === "history"}
            <History {view} {update} />
          {:else if page === "usage"}
            <Usage />
          {:else}
            <About {view} {update} bind:updates />
          {/if}
        {:else if !error}
          <div class="flex flex-col gap-2.5" aria-hidden="true">
            {#each [0, 1, 2] as i (i)}
              <div class="h-16 animate-pulse rounded-xl border border-border/60 bg-muted"></div>
            {/each}
          </div>
        {/if}
      </div>
    {/key}
  </main>
</div>
