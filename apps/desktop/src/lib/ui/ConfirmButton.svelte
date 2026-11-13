<script lang="ts">
  // The one way to delete a single thing: a trash button that asks once. The first press
  // turns it into "Delete?" for a few seconds; the second press does it. Esc, or moving
  // away, puts it back. Anything that erases what the user would have to retype,
  // re-enter or download again goes through this; a removal one click can undo (an X)
  // does not. Deleting everything at once uses ConfirmDialog instead.
  import { Trash2 } from "@lucide/svelte";
  import Button from "./Button.svelte";

  let {
    label,
    text,
    confirm = "Delete?",
    onconfirm,
  }: {
    /** What it deletes, as "Delete <thing>": the button's name and tooltip. */
    label: string;
    /** A visible label, for a text button instead of the trash icon. */
    text?: string;
    confirm?: string;
    onconfirm: () => void;
  } = $props();

  let armed = $state(false);
  let timer = 0;

  function disarm() {
    clearTimeout(timer);
    armed = false;
  }

  function click() {
    if (armed) {
      disarm();
      onconfirm();
      return;
    }
    armed = true;
    timer = window.setTimeout(() => (armed = false), 4000);
  }

  $effect(() => () => clearTimeout(timer));
</script>

<!-- One element throughout, so keyboard focus stays on it between the two presses. The
     icon keeps its place in the row: "Delete?" opens over what is to its left. -->
<span class="relative inline-flex {text ? '' : 'size-7'}">
<Button
  variant={armed ? "danger" : text ? "ghost" : "quiet-danger"}
  size={armed || text ? "sm" : "icon-sm"}
  aria-label={armed ? `${label}: press again to confirm` : text ? undefined : label}
  tooltip={armed || text ? undefined : label}
  style={armed && !text ? "position: absolute; top: 0; right: 0; z-index: 1" : undefined}
  onclick={click}
  onblur={disarm}
  onkeydown={(e) => {
    if (e.key === "Escape" && armed) {
      e.stopPropagation();
      disarm();
    }
  }}
>
  {#if armed}{confirm}{:else if text}{text}{:else}<Trash2 aria-hidden="true" />{/if}
</Button>
</span>
