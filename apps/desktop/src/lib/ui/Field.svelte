<script lang="ts">
  // A labelled field in a form: the label above, the control, then an error or a hint
  // below. The control is the child; give it the same `id` as `for`, and
  // `aria-describedby="{id}-hint"` when there is a hint.
  import type { Snippet } from "svelte";
  import Status from "./Status.svelte";

  let {
    label,
    for: htmlFor,
    hint,
    error,
    children,
  }: { label: string; for: string; hint?: string; error?: string; children: Snippet } = $props();
</script>

<div class="flex min-w-0 flex-col gap-1.5">
  <label for={htmlFor} class="text-xs font-medium text-foreground">{label}</label>
  {@render children()}
  {#if error}
    <Status tone="error">{error}</Status>
  {:else if hint}
    <p id="{htmlFor}-hint" class="text-xs text-muted-foreground">{hint}</p>
  {/if}
</div>
