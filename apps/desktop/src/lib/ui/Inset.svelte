<script lang="ts">
  // A panel set into a card, edge to edge: an add or edit form, a row's extra settings.
  // It opens directly under the row whose button opened it, and that button carries
  // `aria-expanded` and `aria-controls` with this `id`. Fields inside are `field-sm`,
  // buttons `sm`: Cancel is `ghost`, the submit `primary`, both at the end.
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";

  let { as = "div", children, ...rest }: HTMLAttributes<HTMLElement> & { as?: "div" | "form" | "li"; children: Snippet } = $props();
</script>

<svelte:element this={as} class="enter flex flex-col gap-3 bg-inset px-4 py-3.5" {...rest}>
  {@render children()}
</svelte:element>
