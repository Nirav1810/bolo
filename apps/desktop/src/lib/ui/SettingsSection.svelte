<script lang="ts">
  // A titled group of setting rows: the title sits outside, the rows share one card
  // with hairline dividers (T3 Code's SettingsSection + SettingsGroup).
  import type { Snippet } from "svelte";

  let {
    title,
    description,
    action,
    plain = false,
    children,
  }: { title?: string; description?: string; action?: Snippet; plain?: boolean; children: Snippet } = $props();
</script>

<section class="flex flex-col gap-2.5">
  {#if title}
    <div class="flex min-h-7 items-end justify-between gap-4 px-1">
      <div class="flex min-w-0 flex-col gap-0.5">
        <h2 class="text-[15px] font-semibold tracking-[-0.01em] text-foreground">{title}</h2>
        {#if description}<p class="text-xs text-muted-foreground">{description}</p>{/if}
      </div>
      {#if action}<div class="flex shrink-0 items-center gap-1.5">{@render action()}</div>{/if}
    </div>
  {/if}
  {#if plain}
    {@render children()}
  {:else}
    <div
      class="overflow-hidden rounded-xl border border-border/80 bg-card shadow-xs/5 dark:border-border [&>*+*]:border-t [&>*+*]:border-border/60"
    >
      {@render children()}
    </div>
  {/if}
</section>
