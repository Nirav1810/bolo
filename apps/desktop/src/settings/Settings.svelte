<script lang="ts">
  // The settings window: a T3 Code-style sidebar (icon nav in titled groups, the build
  // in the footer) and one centred column of setting sections. Every change is saved
  // and applied at once by Rust (src-tauri/src/commands.rs); there is no Save button.
  import { AppWindow, ArrowUpCircle, AudioLines, BookA, Gauge, History as HistoryIcon, Info, SlidersHorizontal, Sparkles } from "@lucide/svelte";
  import { api, sentence, variant, type Form, type SettingsView, type UpdateView } from "../lib/api";
  import { onAppEvent } from "../lib/events";
  import { PAGES, PAGE_IDS, type Page } from "../lib/pages";
  import Alert from "../lib/ui/Alert.svelte";
  import AppMark from "../lib/ui/AppMark.svelte";
  import NavItem from "../lib/ui/NavItem.svelte";
  import PageHeader from "../lib/ui/PageHeader.svelte";
  import Skeleton from "../lib/ui/Skeleton.svelte";
  import General from "./General.svelte";
  import Engines from "./Engines.svelte";
  import Polish from "./Polish.svelte";
  import Apps from "./Apps.svelte";
  import Dictionary from "./Dictionary.svelte";
  import Snippets from "./Snippets.svelte";
  import History from "./History.svelte";
  import About from "./About.svelte";
  import Usage from "./Usage.svelte";

  let { tab = $bindable("general") }: { tab?: string } = $props();

  const ICONS = {
    general: SlidersHorizontal,
    engines: AudioLines,
    polish: Sparkles,
    apps: AppWindow,
    dictionary: BookA,
    history: HistoryIcon,
    usage: Gauge,
    about: Info,
  } satisfies Record<Page, unknown>;

  const GROUPS: { heading: string; pages: Page[] }[] = [
    { heading: "Dictation", pages: ["general", "engines", "polish", "apps", "dictionary"] },
    { heading: "Activity", pages: ["history", "usage"] },
    { heading: "App", pages: ["about"] },
  ];

  let view = $state<SettingsView | null>(null);
  let error = $state<{ title: string; detail: string } | null>(null);
  let updates = $state<UpdateView | null>(null);

  async function load() {
    // The webview loads with the app, before the app's state is ready: try a few times.
    for (let attempt = 0; ; attempt++) {
      try {
        view = await api.settings();
        if (error?.title === "Couldn’t load settings") error = null;
        return;
      } catch (e) {
        if (attempt >= 20) {
          error = { title: "Couldn’t load settings", detail: sentence(String(e)) };
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
      error = null;
    } catch (e) {
      await load();
      error = { title: "Couldn’t save settings", detail: sentence(String(e)) };
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

  const page = $derived((PAGE_IDS as string[]).includes(tab) ? (tab as Page) : "general");
  /** The sidebar's notice when an update waits: a channel switch names the channel, and a
   *  Nightly version is too long to read there, so it gets no number. */
  const notice = $derived.by(() => {
    const s = updates?.status;
    if (s?.state !== "available") return null;
    const nightly = variant(s.version).nightly;
    if (s.switch) return `Switch to ${nightly ? "Bolo Nightly" : "Bolo Beta"}`;
    return nightly ? "Update available" : `Update to ${s.version}`;
  });

  /** Up, Down, Home and End move through the nav, as in a list. */
  function onNavKey(e: KeyboardEvent) {
    const order = GROUPS.flatMap((g) => g.pages);
    const i = order.indexOf(page);
    const to =
      e.key === "ArrowDown" ? (i + 1) % order.length : e.key === "ArrowUp" ? (i + order.length - 1) % order.length : e.key === "Home" ? 0 : e.key === "End" ? order.length - 1 : -1;
    if (to < 0) return;
    e.preventDefault();
    tab = order[to];
    document.querySelector<HTMLElement>(`[data-page="${order[to]}"]`)?.focus();
  }
</script>

<div class="flex h-screen overflow-hidden bg-background">
  <aside class="flex w-57 flex-none flex-col border-r border-sidebar-border bg-sidebar">
    <div class="px-4 pt-4 pb-3">
      <AppMark version={view?.version ?? updates?.version ?? null} />
    </div>

    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <nav class="flex flex-1 flex-col gap-4 overflow-y-auto px-2 pt-2" aria-label="Settings pages" onkeydown={onNavKey}>
      {#each GROUPS as group (group.heading)}
        <div class="flex flex-col gap-0.5">
          <p class="px-2.5 pb-1 text-xs font-medium text-sidebar-muted-foreground">{group.heading}</p>
          {#each group.pages as id (id)}
            <NavItem icon={ICONS[id]} label={PAGES[id].label} active={page === id} data-page={id} onclick={() => (tab = id)} />
          {/each}
        </div>
      {/each}
    </nav>

    <div class="flex flex-col gap-1 p-2">
      {#if notice}
        <NavItem icon={ArrowUpCircle} label={notice} notice onclick={() => (tab = "about")} />
      {/if}
      {#if view}
        <p class="truncate px-2.5 py-1 text-xs text-sidebar-muted-foreground tabular-nums">Version {view.version}</p>
      {/if}
    </div>
  </aside>

  <main class="min-w-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
    {#key page}
      <div class="enter mx-auto flex w-full max-w-3xl flex-col gap-8 px-8 pt-9 pb-16">
        <PageHeader title={PAGES[page].label} blurb={PAGES[page].blurb} />

        {#if error}<Alert variant="error" title={error.title}>{error.detail}</Alert>{/if}

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
            <Usage onkeys={() => (tab = "engines")} />
          {:else}
            <About {view} {update} bind:updates />
          {/if}
        {:else if !error}
          <Skeleton />
        {/if}
      </div>
    {/key}
  </main>
</div>
