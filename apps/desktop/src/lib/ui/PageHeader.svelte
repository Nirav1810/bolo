<script lang="ts">
  // A page's title and its one line of explanation. `hero` is the larger one, for a
  // step of onboarding.
  import type { Snippet } from "svelte";

  let { title, blurb, size = "page" }: { title: string; blurb?: string | Snippet; size?: "page" | "hero" } = $props();
</script>

<header class="flex flex-col px-1 {size === 'hero' ? 'gap-2' : 'gap-1'}">
  <h1 class="text-foreground {size === 'hero' ? 'text-display' : 'text-title'}">{title}</h1>
  {#if blurb}
    <p class="max-w-xl text-sm text-muted-foreground">
      {#if typeof blurb === "string"}{blurb}{:else}{@render blurb()}{/if}
    </p>
  {/if}
</header>
