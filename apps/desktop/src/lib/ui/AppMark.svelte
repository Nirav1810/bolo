<script lang="ts">
  // The app's icon, name and build badge, the same wherever it appears: the sidebar,
  // onboarding and the debug page (`sm`), and About (`lg`). The name and the icon follow
  // the build, so Bolo Nightly is never taken for Bolo (ADR-0019).
  import { variant } from "../api";
  import Badge from "./Badge.svelte";
  import releaseIcon from "../../../src-tauri/icons/128x128.png?no-inline";
  import nightlyIcon from "../../../src-tauri/icons-nightly/128x128.png?no-inline";

  let { version, size = "sm" }: { version: string | null; size?: "sm" | "lg" } = $props();

  const build = $derived(version === null ? null : variant(version));
  const px = $derived(size === "lg" ? 40 : 24);
</script>

<div class="flex min-w-0 items-center {size === 'lg' ? 'gap-3' : 'gap-2.5'}">
  <img src={build?.nightly ? nightlyIcon : releaseIcon} width={px} height={px} alt="" class={size === "lg" ? "rounded-xl" : "rounded-md"} />
  <span class="truncate text-foreground {size === 'lg' ? 'text-heading' : 'text-sm font-semibold'}">{build?.product ?? "Bolo"}</span>
  {#if build}<Badge variant={build.nightly ? "nightly" : "neutral"}>{build.name}</Badge>{/if}
</div>
