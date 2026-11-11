<script lang="ts">
  import type { Snippet } from "svelte";
  let { children, class: className = "" }: { children: Snippet; class?: string } = $props();
</script>

<kbd
  class="inline-flex h-5 min-w-5 select-none items-center justify-center rounded border border-b-2 border-foreground/15 px-1.5 font-sans text-xs font-medium text-foreground {className}"
  >{@render children()}</kbd
>
