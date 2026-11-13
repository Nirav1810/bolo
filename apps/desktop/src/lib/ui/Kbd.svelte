<script lang="ts">
  // A key on the keyboard, set in running text.
  import type { Snippet } from "svelte";
  let { children, class: className = "" }: { children: Snippet; class?: string } = $props();
</script>

<kbd
  class="inline-flex h-4.5 min-w-4.5 items-center justify-center rounded-md border border-input bg-control px-1.5 align-baseline text-xs font-medium text-foreground shadow-control select-none {className}"
  >{@render children()}</kbd
>
