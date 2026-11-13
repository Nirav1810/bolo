<script lang="ts">
  // A switch: 30 × 18, primary when on, the thumb stretches while pressed. Its hit area
  // reaches 6 px past the track on every side, so it is an easy 42 × 30 target.
  import type { HTMLButtonAttributes } from "svelte/elements";

  let {
    checked,
    onchange,
    disabled = false,
    ...rest
  }: Omit<HTMLButtonAttributes, "onchange"> & { checked: boolean; onchange: (checked: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  {disabled}
  class="group/switch focus-ring disabled-dim relative inline-flex h-4.5 w-7.5 shrink-0 items-center rounded-full p-0.5 transition-[background-color] after:absolute after:-inset-1.5 {checked
    ? 'bg-primary'
    : 'bg-switch-off'}"
  onclick={() => onchange(!checked)}
  {...rest}
>
  <span
    class="pointer-events-none block size-3.5 rounded-full bg-switch-thumb shadow-control transition-[translate,scale] group-active/switch:scale-x-115 {checked
      ? 'origin-right translate-x-3'
      : 'origin-left translate-x-0'}"
  ></span>
</button>
