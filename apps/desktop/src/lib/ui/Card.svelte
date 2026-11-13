<script lang="ts">
  // The one card surface: a 12 px corner, a hairline edge, the card colour. `divided`
  // draws a hairline between its children, for a list of rows.
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { cn } from "./cn";

  let {
    as = "div",
    divided = false,
    class: className = "",
    children,
    ...rest
  }: Omit<HTMLAttributes<HTMLElement>, "class"> & {
    as?: "div" | "ul" | "ol" | "section";
    divided?: boolean;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<svelte:element
  this={as}
  class={cn(
    "overflow-hidden rounded-xl border border-border bg-card shadow-card",
    divided && "[&>*+*]:border-t [&>*+*]:border-divider",
    className,
  )}
  {...rest}
>
  {@render children()}
</svelte:element>
