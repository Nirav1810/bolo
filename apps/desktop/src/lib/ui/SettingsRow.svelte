<script lang="ts">
  // One row of a card: an optional icon tile, a title with badges, a description and a
  // status line on the left; its control on the right; and optional content below
  // (a progress bar), lined up with the text. Every row in a card is one of these, so
  // padding, gaps and alignment are decided here and nowhere else.
  import type { Snippet } from "svelte";
  import Status from "./Status.svelte";

  let {
    title,
    labelFor,
    titleId,
    description,
    badges,
    status,
    control,
    leading,
    align = "center",
    as = "div",
    children,
  }: {
    title: string | Snippet;
    /** The id of the row's one control: the title becomes its `<label>`. */
    labelFor?: string;
    titleId?: string;
    description?: string | Snippet;
    badges?: Snippet;
    /** How the row's thing is doing, or how the last action on it went. */
    status?: { tone: "success" | "warning" | "error" | "busy"; text: string } | null;
    /** `sm` buttons, a Switch, a Segmented or a Select. */
    control?: Snippet;
    /** An IconTile. */
    leading?: Snippet;
    /** `start` when the text runs to several lines (a snippet, a dictation). */
    align?: "center" | "start";
    as?: "div" | "li";
    children?: Snippet;
  } = $props();
</script>

<svelte:element this={as} class="px-4 py-3">
  <div class="flex gap-3 {align === 'start' ? 'items-start' : 'items-center'}">
    {#if leading}{@render leading()}{/if}
    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
      <div class="flex min-h-5 flex-wrap items-center gap-x-1.5 gap-y-1">
        {#if typeof title !== "string"}
          <div id={titleId} class="min-w-0 text-sm font-medium text-foreground">{@render title()}</div>
        {:else if labelFor}
          <label for={labelFor} id={titleId} class="text-sm font-medium text-foreground">{title}</label>
        {:else}
          <p id={titleId} class="text-sm font-medium text-foreground">{title}</p>
        {/if}
        {@render badges?.()}
      </div>
      {#if description}
        <div class="max-w-xl text-xs text-pretty text-muted-foreground">
          {#if typeof description === "string"}{description}{:else}{@render description()}{/if}
        </div>
      {/if}
      {#if status}<Status tone={status.tone}>{status.text}</Status>{/if}
    </div>
    {#if control}<div class="flex shrink-0 items-center gap-1.5">{@render control()}</div>{/if}
  </div>
  {#if children}<div class="empty:hidden mt-2.5 {leading ? 'pl-11' : ''}">{@render children()}</div>{/if}
</svelte:element>
