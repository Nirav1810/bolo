<script lang="ts" module>
  export type ButtonVariant = "primary" | "outline" | "ghost" | "ghost-muted" | "ghost-destructive" | "destructive-outline" | "secondary";
  export type ButtonSize = "default" | "sm" | "xs" | "icon" | "icon-sm" | "icon-xs";
</script>

<script lang="ts">
  // A button in T3 Code's style: 8 px radius, a 0.97 press, an inset highlight on primary.
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import { cn } from "./cn";

  let {
    variant = "outline",
    size = "default",
    class: className = "",
    type = "button",
    children,
    ...rest
  }: Omit<HTMLButtonAttributes, "class"> & { class?: string; variant?: ButtonVariant; size?: ButtonSize; children?: Snippet } = $props();

  const base =
    "relative inline-flex shrink-0 select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border font-medium text-sm outline-none transition-[background-color,border-color,color,box-shadow,scale] duration-150 ease-out-ui active:not-disabled:scale-[0.97] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-55 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4";

  const sizes: Record<ButtonSize, string> = {
    default: "h-8 px-[11px]",
    sm: "h-7 gap-1 px-[9px] text-[13px] [&_svg:not([class*='size-'])]:size-3.5",
    xs: "h-6 gap-1 rounded-md px-[7px] text-xs [&_svg:not([class*='size-'])]:size-3.5",
    icon: "size-8",
    "icon-sm": "size-7 [&_svg:not([class*='size-'])]:size-3.5",
    "icon-xs": "size-6 rounded-md [&_svg:not([class*='size-'])]:size-3.5",
  };

  const variants: Record<ButtonVariant, string> = {
    primary:
      "border-primary bg-primary text-primary-foreground shadow-xs shadow-primary/24 inset-shadow-[0_1px_rgb(255_255_255/16%)] hover:not-disabled:bg-primary/90 active:inset-shadow-[0_1px_rgb(0_0_0/8%)]",
    outline:
      "border-input bg-popover text-foreground shadow-xs/5 hover:not-disabled:bg-accent/60 dark:bg-input/32 dark:hover:not-disabled:bg-input/64 [&_svg]:text-muted-foreground",
    secondary: "border-transparent bg-secondary text-secondary-foreground hover:not-disabled:bg-secondary/70",
    ghost: "border-transparent text-foreground hover:not-disabled:bg-accent [&_svg]:text-muted-foreground",
    "ghost-muted": "border-transparent text-muted-foreground hover:not-disabled:bg-accent hover:not-disabled:text-foreground",
    "ghost-destructive":
      "border-transparent text-muted-foreground hover:not-disabled:bg-destructive/8 hover:not-disabled:text-destructive-foreground",
    "destructive-outline":
      "border-input bg-popover text-destructive-foreground shadow-xs/5 hover:not-disabled:border-destructive/32 hover:not-disabled:bg-destructive/6 dark:bg-input/32",
  };
</script>

<button {type} class={cn(base, sizes[size], variants[variant], className)} {...rest}>{@render children?.()}</button>
