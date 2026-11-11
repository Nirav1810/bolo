<script lang="ts">
  // One choice out of a few: a muted track with the chosen segment raised.
  let {
    label,
    value,
    options,
    onchange,
    size = "default",
  }: {
    label: string;
    value: string;
    options: [string, string][];
    onchange: (value: string) => void;
    size?: "default" | "sm";
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

<div class="inline-flex shrink-0 gap-0.5 rounded-lg bg-accent p-0.5 dark:bg-input/40" role="radiogroup" aria-label={label} tabindex="-1" {onkeydown}>
  {#each options as [v, text] (v)}
    <button
      type="button"
      role="radio"
      aria-checked={value === v}
      tabindex={value === v ? 0 : -1}
      class="rounded-md px-3 font-medium outline-none transition-[background-color,color,box-shadow] duration-150 focus-visible:ring-2 focus-visible:ring-ring {size === 'sm'
        ? 'h-6 text-xs'
        : 'h-7 text-[13px]'} {value === v
        ? 'bg-card text-foreground shadow-xs/10 ring-1 ring-black/5 dark:bg-white/12 dark:ring-white/6'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => value !== v && onchange(v)}>{text}</button
    >
  {/each}
</div>
