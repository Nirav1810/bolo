<script lang="ts">
  // The main window's pages, chosen by the URL hash, which Rust sets from the tray:
  // `#settings`, a settings tab (`#history`, `#engines`, ...) and `#debug`.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Settings from "./settings/Settings.svelte";
  import Debug from "./Debug.svelte";

  const TABS = ["general", "engines", "polish", "dictionary", "history", "about"];
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

  $effect(() => {
    getCurrentWindow().setTitle(route === "debug" ? "Bolo debug" : "Bolo Settings");
  });
</script>

{#if route === "debug"}
  <Debug />
{:else}
  <Settings bind:tab />
{/if}
