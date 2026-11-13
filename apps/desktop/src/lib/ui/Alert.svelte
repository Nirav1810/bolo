<script lang="ts">
  // A block message with its icon, tinted by kind (never colour alone). It goes first
  // under the page header when it is about the page, or in a SettingsSection's `footer`
  // when it is about that section. An error is announced; pass `live` for anything else
  // that appears because of what the user just did.
  import type { Snippet } from "svelte";
  import { CircleAlert, CircleCheck, Info, TriangleAlert } from "@lucide/svelte";

  type Variant = "info" | "warning" | "error" | "success";
  let {
    variant = "info",
    title,
    live = false,
    children,
    actions,
  }: {
    variant?: Variant;
    title?: string;
    live?: boolean;
    children?: Snippet;
    /** Buttons, all `size="sm"`: the first `outline`, the rest `ghost`. */
    actions?: Snippet;
  } = $props();

  const surface: Record<Variant, string> = {
    info: "border-border bg-inset",
    warning: "border-warning-edge bg-warning-tint",
    error: "border-destructive-edge bg-destructive-tint",
    success: "border-success-edge bg-success-tint",
  };
  const colour: Record<Variant, string> = {
    info: "text-info-foreground",
    warning: "text-warning-foreground",
    error: "text-destructive-foreground",
    success: "text-success-foreground",
  };
  const ICONS = { info: Info, warning: TriangleAlert, error: CircleAlert, success: CircleCheck };
  const Icon = $derived(ICONS[variant]);
</script>

<div
  class="flex items-start gap-2.5 rounded-xl border px-3.5 py-3 text-sm {surface[variant]}"
  role={variant === "error" ? "alert" : live ? "status" : undefined}
>
  <Icon class="mt-0.5 size-4 shrink-0 {colour[variant]}" aria-hidden="true" />
  <div class="flex min-w-0 flex-1 flex-col gap-0.5">
    {#if title}<p class="font-medium text-foreground">{title}</p>{/if}
    {#if children}<div class="text-pretty break-words {title ? 'text-muted-foreground' : 'text-foreground'}">{@render children()}</div>{/if}
    {#if actions}<div class="mt-2 flex flex-wrap gap-2">{@render actions()}</div>{/if}
  </div>
</div>
