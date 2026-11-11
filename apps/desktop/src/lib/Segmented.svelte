<script lang="ts">
  // A Windows-style segmented choice: one pressed button out of a few.
  let {
    label,
    value,
    options,
    onchange,
  }: { label: string; value: string; options: [string, string][]; onchange: (value: string) => void } = $props();
</script>

<div class="segmented" role="group" aria-label={label}>
  {#each options as [v, text]}
    <button type="button" aria-pressed={value === v} onclick={() => value !== v && onchange(v)}>{text}</button>
  {/each}
</div>
