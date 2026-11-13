<script lang="ts">
  // A titled group: the title sits outside, the rows share one card with hairlines
  // between them (T3 Code's SettingsSection + SettingsGroup). `footer` is for an Alert
  // about this section; it sits under the card.
  import type { Snippet } from "svelte";
  import Card from "./Card.svelte";

  let {
    title,
    description,
    action,
    footer,
    as = "div",
    label,
    children,
  }: {
    title?: string;
    description?: string;
    /** `ghost sm` buttons (and a count) at the right of the title. */
    action?: Snippet;
    footer?: Snippet;
    /** `ul` when every child is a `SettingsRow as="li"`. */
    as?: "div" | "ul";
    /** The list's name for a screen reader when there is no `title`. */
    label?: string;
    children: Snippet;
  } = $props();
</script>

<section class="flex flex-col gap-2.5">
  {#if title}
    <div class="flex min-h-7 items-end justify-between gap-4 px-1">
      <div class="flex min-w-0 flex-col gap-0.5">
        <h2 class="text-heading text-foreground">{title}</h2>
        {#if description}<p class="text-xs text-muted-foreground">{description}</p>{/if}
      </div>
      {#if action}<div class="flex shrink-0 items-center gap-1.5">{@render action()}</div>{/if}
    </div>
  {/if}
  <Card {as} divided aria-label={title ? undefined : label}>
    {@render children()}
  </Card>
  {@render footer?.()}
</section>
