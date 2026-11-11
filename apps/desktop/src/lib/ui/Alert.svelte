<script lang="ts">
  // An inline message with its icon, tinted by kind (never colour alone).
  import type { Snippet } from "svelte";
  import { CircleAlert, Info, TriangleAlert, CircleCheck } from "@lucide/svelte";

  let {
    variant = "info",
    title,
    children,
    actions,
    class: className = "",
  }: {
    variant?: "info" | "warning" | "error" | "success";
    title?: string;
    children?: Snippet;
    actions?: Snippet;
    class?: string;
  } = $props();

  const tone = {
    info: "border-border bg-muted/60 [&>svg]:text-info-foreground dark:bg-white/[0.02]",
    warning: "border-warning/30 bg-warning/6 [&>svg]:text-warning-foreground dark:bg-warning/8",
    error: "border-destructive/30 bg-destructive/5 [&>svg]:text-destructive-foreground dark:bg-destructive/8",
    success: "border-success/30 bg-success/6 [&>svg]:text-success-foreground dark:bg-success/8",
  };
  const ICONS = { info: Info, warning: TriangleAlert, error: CircleAlert, success: CircleCheck };
  const Icon = $derived(ICONS[variant]);
</script>

<div
  class="flex items-start gap-2.5 rounded-xl border px-3.5 py-3 text-[13px] leading-5 {tone[variant]} {className}"
  role={variant === "error" || variant === "warning" ? "alert" : "status"}
>
  <Icon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
  <div class="flex min-w-0 flex-1 flex-col gap-1">
    {#if title}<p class="font-medium text-foreground">{title}</p>{/if}
    {#if children}<div class="text-muted-foreground [&_strong]:font-medium [&_strong]:text-foreground">{@render children()}</div>{/if}
    {#if actions}<div class="mt-1.5 flex flex-wrap gap-2">{@render actions()}</div>{/if}
  </div>
</div>
