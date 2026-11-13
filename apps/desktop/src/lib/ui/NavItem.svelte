<script lang="ts">
  // One row of the sidebar: an icon and a page's name. The current page's row is raised
  // and its icon takes the primary colour. `notice` is the row that says an update is
  // waiting.
  import type { Component } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  let {
    icon: Icon,
    label,
    active = false,
    notice = false,
    ...rest
  }: Omit<HTMLButtonAttributes, "class"> & {
    icon: Component<{ class?: string }>;
    label: string;
    active?: boolean;
    notice?: boolean;
  } = $props();

  const look = $derived(
    notice
      ? "border-primary-edge bg-primary-tint text-sidebar-foreground"
      : active
        ? "border-sidebar-row-edge bg-sidebar-row-selected text-sidebar-foreground shadow-card"
        : "border-transparent text-sidebar-muted-foreground hover:bg-sidebar-row-hover hover:text-sidebar-foreground",
  );
</script>

<button
  type="button"
  class="focus-ring flex h-8 w-full items-center gap-2.5 rounded-lg border px-2.5 text-left text-sm font-medium transition-colors {look}"
  aria-current={active ? "page" : undefined}
  {...rest}
>
  <span class="contents" aria-hidden="true"><Icon class="size-4 shrink-0 {active || notice ? 'text-primary-text' : ''}" /></span>
  <span class="truncate">{label}</span>
</button>
