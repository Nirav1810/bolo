<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "./cn";

  type Variant = "secondary" | "outline" | "success" | "info" | "warning" | "error" | "primary" | "nightly";
  let {
    variant = "secondary",
    class: className = "",
    children,
  }: { variant?: Variant; class?: string; children: Snippet } = $props();

  const variants: Record<Variant, string> = {
    secondary: "border-transparent bg-accent text-muted-foreground",
    outline: "border-border bg-transparent text-muted-foreground",
    success: "border-transparent bg-success/10 text-success-foreground dark:bg-success/16",
    info: "border-transparent bg-info/10 text-info-foreground dark:bg-info/16",
    warning: "border-transparent bg-warning/12 text-warning-foreground dark:bg-warning/16",
    error: "border-transparent bg-destructive/10 text-destructive-foreground dark:bg-destructive/16",
    primary: "border-transparent bg-primary/10 text-primary-text dark:bg-primary/20",
    nightly: "border-transparent bg-nightly/12 text-nightly-foreground dark:bg-nightly/18",
  };
</script>

<span
  class={cn(
    "inline-flex h-[18px] shrink-0 items-center gap-1 whitespace-nowrap rounded-[5px] border px-1.5 text-2xs font-medium [&_svg]:size-3",
    variants[variant],
    className,
  )}>{@render children()}</span
>
