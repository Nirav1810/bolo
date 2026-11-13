<script lang="ts">
  // One item of a set that is shown as a word (a dictionary word), with its X.
  import { X } from "@lucide/svelte";
  import Button from "./Button.svelte";

  let { label, onremove }: { label: string; onremove?: () => void } = $props();
</script>

<li
  class="inline-flex h-7 items-center gap-0.5 rounded-lg border border-input bg-control pl-2.5 text-sm font-medium text-foreground shadow-control {onremove
    ? 'pr-0.5'
    : 'pr-2.5'}"
>
  {label}
  {#if onremove}
    <Button variant="quiet" size="icon-xs" aria-label="Remove {label}" onclick={onremove}><X aria-hidden="true" /></Button>
  {/if}
</li>
