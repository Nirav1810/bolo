<script lang="ts">
  // A destructive icon button that asks once: the first click turns it into a labelled
  // "Delete?" button for a few seconds, the second click does it.
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  let {
    label,
    confirm = "Delete?",
    onconfirm,
    children,
  }: { label: string; confirm?: string; onconfirm: () => void; children: Snippet } = $props();

  let armed = $state(false);
  let timer = 0;

  function click() {
    if (armed) {
      clearTimeout(timer);
      armed = false;
      onconfirm();
      return;
    }
    armed = true;
    timer = window.setTimeout(() => (armed = false), 3000);
  }

  $effect(() => () => clearTimeout(timer));
</script>

<!-- One element throughout, so keyboard focus stays on it between the two presses. -->
<Button
  variant={armed ? "destructive-outline" : "ghost-destructive"}
  size={armed ? "sm" : "icon-sm"}
  aria-label={armed ? `${label}: press again to confirm` : label}
  title={label}
  onclick={click}
  onblur={() => (armed = false)}
>
  {#if armed}{confirm}{:else}{@render children()}{/if}
</Button>
