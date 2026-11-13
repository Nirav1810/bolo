<script lang="ts">
  // One choice out of a few: a trough with the chosen segment raised and outlined.
  // 28 px inside a card, like every control there; `default` (32 px) in a page's own bar.
  let {
    label,
    value,
    options,
    onchange,
    size = "sm",
  }: {
    label: string;
    value: string;
    options: [string, string][];
    onchange: (value: string) => void;
    size?: "sm" | "default";
  } = $props();

  /** Arrow keys move the choice, as in any radio group; only the chosen one is a tab stop. */
  function onkeydown(e: KeyboardEvent) {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const i = options.findIndex(([v]) => v === value);
    const [next] = options[(i + step + options.length) % options.length];
    onchange(next);
    const group = e.currentTarget as HTMLElement;
    requestAnimationFrame(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }
</script>

<div class="inline-flex shrink-0 gap-0.5 rounded-lg bg-track p-0.5 {size === 'sm' ? 'h-7' : 'h-8'}" role="radiogroup" aria-label={label} tabindex="-1" {onkeydown}>
  {#each options as [v, text] (v)}
    <button
      type="button"
      role="radio"
      aria-checked={value === v}
      tabindex={value === v ? 0 : -1}
      class="focus-ring rounded-md border text-sm font-medium transition-[background-color,border-color,color,box-shadow] {size === 'sm'
        ? 'h-6 px-2.5'
        : 'h-7 px-3'} {value === v
        ? 'border-field-edge bg-track-thumb text-foreground shadow-control'
        : 'border-transparent text-muted-foreground hover:text-foreground'}"
      onclick={() => value !== v && onchange(v)}>{text}</button
    >
  {/each}
</div>
