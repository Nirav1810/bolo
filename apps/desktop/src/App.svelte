<script lang="ts">
  // The main window's pages, chosen by the URL hash, which Rust sets from the tray:
  // `#settings`, a settings tab (`#history`, `#engines`, ...), `#onboarding` and `#debug`.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api } from "./lib/api";
  import { PAGE_IDS } from "./lib/pages";
  import Settings from "./settings/Settings.svelte";
  import Debug from "./Debug.svelte";
  import Onboarding from "./onboarding/Onboarding.svelte";

  const TABS: string[] = PAGE_IDS;
  const initial = location.hash.slice(1) || "settings";
  let route = $state(initial);
  let tab = $state(TABS.includes(initial) ? initial : "general");

  $effect(() => {
    const onHash = () => {
      route = location.hash.slice(1) || "settings";
      if (TABS.includes(route)) tab = route;
    };
    addEventListener("hashchange", onHash);
    return () => removeEventListener("hashchange", onHash);
  });

  // "Bolo Nightly settings" for the nightly app, so it is never taken for Bolo.
  let product = $state("Bolo");
  $effect(() => {
    api
      .updateView()
      .then((v) => (product = v.product))
      .catch(() => {});
  });

  $effect(() => {
    const titles: Record<string, string> = { debug: `${product} debug`, onboarding: `Welcome to ${product}` };
    getCurrentWindow().setTitle(titles[route] ?? `${product} settings`);
  });
</script>

{#if route === "debug"}
  <Debug />
{:else if route === "onboarding"}
  <Onboarding ondone={() => (location.hash = "settings")} />
{:else}
  <Settings bind:tab />
{/if}
