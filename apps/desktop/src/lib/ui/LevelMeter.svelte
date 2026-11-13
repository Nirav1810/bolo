<script lang="ts">
  // The microphone's level as a row of bars, lit from the left. It reads the level the
  // same way the pill's waveform does (lib/waveform.ts), in the same colour.
  import { loudness } from "../waveform";

  let { rms, label = "Microphone level" }: { rms: number; label?: string } = $props();

  const BARS = 32;
  const level = $derived(loudness(rms));
</script>

<div class="flex h-1.5 items-stretch gap-0.5" role="meter" aria-label={label} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(level * 100)}>
  {#each Array(BARS) as _, i (i)}
    <span class="flex-1 rounded-full {i / BARS < level ? 'bg-primary-text' : 'bg-track'}"></span>
  {/each}
</div>
