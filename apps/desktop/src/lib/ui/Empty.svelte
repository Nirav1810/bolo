<script lang="ts">
  // An empty list: an icon in a soft square, a line, and a hint.
  import type { Component, Snippet } from "svelte";
  let { icon: Icon, title, hint, children }: { icon: Component<{ class?: string }>; title: string; hint?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center gap-2 px-6 py-10 text-center">
  <div class="mb-1 grid size-10 place-items-center rounded-xl border border-border bg-muted shadow-xs/5">
    <Icon class="size-[18px] text-muted-foreground" />
  </div>
  <p class="text-sm font-medium text-foreground">{title}</p>
  {#if hint}<p class="max-w-xs text-xs text-muted-foreground">{hint}</p>{/if}
  {@render children?.()}
</div>
