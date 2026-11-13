<script lang="ts">
  // Asks before something that can't be undone and covers more than one thing (delete
  // all history, reset usage). The button that opens it ends with "…"; the title is the
  // question; Cancel has the focus. A single thing is deleted with ConfirmButton.
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  let {
    open = $bindable(false),
    title,
    confirm,
    onconfirm,
    children,
  }: {
    open?: boolean;
    /** The question: "Delete all dictations?" */
    title: string;
    /** The button that does it: "Delete all". */
    confirm: string;
    onconfirm: () => void;
    /** One sentence on what is lost. */
    children?: Snippet;
  } = $props();

  const uid = $props.id();
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialog}
  class="dialog"
  aria-labelledby="{uid}-title"
  aria-describedby={children ? `${uid}-body` : undefined}
  onclose={() => (open = false)}
  onclick={(e) => e.target === dialog && (open = false)}
>
  <div class="flex flex-col gap-1.5 p-5">
    <h2 id="{uid}-title" class="text-heading text-foreground">{title}</h2>
    {#if children}<p id="{uid}-body" class="text-sm text-muted-foreground">{@render children()}</p>{/if}
  </div>
  <div class="flex justify-end gap-2 border-t border-divider bg-inset px-5 py-3">
    <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
    <Button
      variant="danger"
      onclick={() => {
        open = false;
        onconfirm();
      }}>{confirm}</Button
    >
  </div>
</dialog>
