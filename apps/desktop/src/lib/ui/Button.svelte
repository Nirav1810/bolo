<script lang="ts" module>
  /**
   * Which variant for which job:
   * - `primary`: a form's submit, or the one forward action of a step (Save, Add, Continue,
   *   Update now).
   * - `outline`: a row's own action (Download, Make primary, Replace…, Check now), and the
   *   first action of an Alert or an Empty.
   * - `ghost`: the lesser action beside one of those (Check, Cancel, Back), a section
   *   header's action, and links out (label first, then `ExternalLink`).
   * - `quiet`: an icon button, grey until hovered (Copy, Edit, the expand chevron, X).
   * - `danger`: a destructive action being confirmed. Only ConfirmButton and
   *   ConfirmDialog use it.
   * - `quiet-danger`: a destructive icon button before it is confirmed (ConfirmButton).
   */
  export type ButtonVariant = "primary" | "outline" | "ghost" | "quiet" | "danger" | "quiet-danger";
  /**
   * `sm` (28 px) inside a card: a settings row, a section header, an Alert, an Empty, an
   * Inset. `default` (32 px) in a page's own bar, a dialog, the onboarding footer, or
   * beside a 32 px field. Icon buttons: `icon-sm` in a row, `icon` beside a 32 px
   * field, `icon-xs` (24 px) only inside another control (a chip).
   */
  export type ButtonSize = "default" | "sm" | "icon" | "icon-sm" | "icon-xs";
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import { LoaderCircle } from "@lucide/svelte";
  import { cn } from "./cn";
  import { tooltip as tip } from "./tooltip";

  let {
    variant = "outline",
    size = "default",
    class: className = "",
    type = "button",
    tooltip,
    loading = false,
    disabled,
    children,
    ...rest
  }: Omit<HTMLButtonAttributes, "class"> & {
    class?: string;
    variant?: ButtonVariant;
    size?: ButtonSize;
    /** Shown on hover and keyboard focus. An icon button shows its `aria-label` by itself. */
    tooltip?: string;
    /** The work this button started is still running: it spins and can't be pressed. */
    loading?: boolean;
    children?: Snippet;
  } = $props();

  const base =
    "focus-ring disabled-dim relative inline-flex shrink-0 select-none items-center justify-center gap-1.5 border text-sm font-medium whitespace-nowrap transition-[background-color,border-color,color,box-shadow,scale] motion-safe:active:not-disabled:scale-[0.97] [&_svg]:pointer-events-none [&_svg]:shrink-0";

  const sizes: Record<ButtonSize, string> = {
    default: "h-8 rounded-lg px-3 [&_svg]:size-4",
    sm: "h-7 rounded-lg px-2.5 [&_svg]:size-4",
    icon: "size-8 rounded-lg [&_svg]:size-4",
    "icon-sm": "size-7 rounded-lg [&_svg]:size-4",
    "icon-xs": "size-6 rounded-md [&_svg]:size-3.5",
  };

  const variants: Record<ButtonVariant, string> = {
    primary:
      "border-primary bg-primary text-primary-foreground shadow-primary hover:not-disabled:border-primary-hover hover:not-disabled:bg-primary-hover",
    outline: "border-input bg-control text-foreground shadow-control hover:not-disabled:bg-control-hover [&_svg]:text-muted-foreground",
    ghost: "border-transparent text-foreground hover:not-disabled:bg-accent [&_svg]:text-muted-foreground",
    quiet: "border-transparent text-muted-foreground hover:not-disabled:bg-accent hover:not-disabled:text-foreground",
    danger: "border-destructive-edge bg-control text-destructive-foreground shadow-control hover:not-disabled:bg-destructive-tint",
    "quiet-danger":
      "border-transparent text-muted-foreground hover:not-disabled:bg-destructive-tint hover:not-disabled:text-destructive-foreground",
  };

  const iconOnly = $derived(size === "icon" || size === "icon-sm" || size === "icon-xs");
  const tipText = $derived(tooltip ?? (iconOnly ? (rest["aria-label"] ?? undefined) : undefined));
</script>

<button
  {type}
  class={cn(base, sizes[size], variants[variant], loading && "[&>svg~svg]:hidden", className)}
  disabled={disabled || loading}
  aria-busy={loading || undefined}
  use:tip={tipText}
  {...rest}
>
  {#if loading}<LoaderCircle class="animate-spin" aria-hidden="true" />{/if}{@render children?.()}
</button>
