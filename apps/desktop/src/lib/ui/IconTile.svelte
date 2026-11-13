<script lang="ts">
  // The square an icon sits in at the start of a row (32 px), or above a title (`lg`,
  // 40 px). Grey by default; a tone when the row is in that state.
  import type { Component } from "svelte";

  type Tone = "neutral" | "primary" | "success" | "warning" | "error";
  let {
    icon: Icon,
    tone = "neutral",
    size = "default",
    spin = false,
  }: { icon: Component<{ class?: string }>; tone?: Tone; size?: "default" | "lg"; spin?: boolean } = $props();

  const tones: Record<Tone, string> = {
    neutral: "bg-muted text-muted-foreground",
    primary: "bg-primary-tint text-primary-text",
    success: "bg-success-tint text-success-foreground",
    warning: "bg-warning-tint text-warning-foreground",
    error: "bg-destructive-tint text-destructive-foreground",
  };
</script>

<div class="grid shrink-0 place-items-center {size === 'lg' ? 'size-10 rounded-xl' : 'size-8 rounded-lg'} {tones[tone]}" aria-hidden="true">
  <Icon class="size-4 {spin ? 'animate-spin' : ''}" />
</div>
