<script lang="ts">
  // A progress bar with its caption. `value` is 0–100, or `null` when the amount isn't
  // known (the bar then fills and pulses).
  let { label, value, caption }: { label: string; value: number | null; caption?: string } = $props();
</script>

<div class="flex flex-col gap-1.5">
  <div
    class="h-1 overflow-hidden rounded-full bg-track"
    role="progressbar"
    aria-label={label}
    aria-valuenow={value ?? undefined}
    aria-valuemin="0"
    aria-valuemax="100"
  >
    <div class="h-full rounded-full bg-primary transition-[width] duration-(--duration-base) {value === null ? 'animate-pulse' : ''}" style:width="{value ?? 100}%"></div>
  </div>
  {#if caption}<p class="text-xs text-muted-foreground tabular-nums">{caption}</p>{/if}
</div>
