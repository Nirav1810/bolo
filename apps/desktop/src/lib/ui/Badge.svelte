<script lang="ts">
  // A small label on a row: what something is (`neutral`, `outline`) or the state it is
  // in (the tinted ones). 20 px tall, 12 px text.
  import type { Snippet } from "svelte";
  import { cn } from "./cn";

  type Variant = "neutral" | "outline" | "primary" | "success" | "info" | "warning" | "error" | "nightly";
  let {
    variant = "neutral",
    class: className = "",
    children,
  }: { variant?: Variant; class?: string; children: Snippet } = $props();

  const variants: Record<Variant, string> = {
    neutral: "border-transparent bg-muted text-muted-foreground",
    outline: "border-border text-muted-foreground",
    primary: "border-transparent bg-primary-tint text-primary-text",
    success: "border-transparent bg-success-tint text-success-foreground",
    info: "border-transparent bg-info-tint text-info-foreground",
    warning: "border-transparent bg-warning-tint text-warning-foreground",
    error: "border-transparent bg-destructive-tint text-destructive-foreground",
    nightly: "border-transparent bg-nightly-tint text-nightly-foreground",
  };
</script>

<span class={cn("inline-flex h-5 shrink-0 items-center rounded-md border px-1.5 text-xs font-medium whitespace-nowrap", variants[variant], className)}
  >{@render children()}</span
>
