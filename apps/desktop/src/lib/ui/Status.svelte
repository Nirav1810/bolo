<script lang="ts">
  // One line that says how something went, with its icon (never colour alone): under a
  // row's description, under a field, or on its own (`sm`, in the body size). `busy`
  // spins while a check runs.
  import type { Snippet } from "svelte";
  import { CircleAlert, CircleCheck, LoaderCircle, TriangleAlert } from "@lucide/svelte";

  type Tone = "success" | "warning" | "error" | "busy";
  let { tone, size = "xs", children }: { tone: Tone; size?: "xs" | "sm"; children: Snippet } = $props();

  const ICONS = { success: CircleCheck, warning: TriangleAlert, error: CircleAlert, busy: LoaderCircle };
  const colour: Record<Tone, string> = {
    success: "text-success-foreground",
    warning: "text-warning-foreground",
    error: "text-destructive-foreground",
    busy: "text-muted-foreground",
  };
  const Icon = $derived(ICONS[tone]);
  // Success and busy read in the text colour; a warning or an error takes its own.
  const text = $derived(tone === "warning" || tone === "error" ? colour[tone] : size === "sm" ? "text-foreground" : "text-muted-foreground");
</script>

<p class="flex items-start gap-1.5 {size === 'sm' ? 'text-sm' : 'text-xs'} {text}" role={tone === "error" ? "alert" : "status"}>
  <Icon
    class="shrink-0 {size === 'sm' ? 'mt-0.5 size-4' : 'mt-0.5 size-3.5'} {colour[tone]} {tone === 'busy' ? 'animate-spin' : ''}"
    aria-hidden="true"
  />
  <span class="min-w-0">{@render children()}</span>
</p>
