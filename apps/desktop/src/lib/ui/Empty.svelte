<script lang="ts">
  // An empty list or page: an icon tile, a title ("No <things> yet") and one sentence.
  // Always inside a card (a SettingsSection's body, or a Card). Its buttons are `sm`.
  import type { Component, Snippet } from "svelte";
  import IconTile from "./IconTile.svelte";

  let { icon, title, hint, children }: { icon: Component<{ class?: string }>; title: string; hint?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center px-6 py-10 text-center">
  <IconTile {icon} size="lg" />
  <p class="mt-3 text-sm font-medium text-foreground">{title}</p>
  {#if hint}<p class="mt-0.5 max-w-xs text-xs text-muted-foreground">{hint}</p>{/if}
  {#if children}<div class="mt-4 flex flex-wrap justify-center gap-2">{@render children()}</div>{/if}
</div>
