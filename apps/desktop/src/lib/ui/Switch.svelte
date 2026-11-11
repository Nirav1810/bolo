<script lang="ts">
  // T3 Code's switch: 30 × 18, primary when on, the thumb stretches while pressed.
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
  class="group/switch inline-flex h-[18px] w-[30px] shrink-0 items-center rounded-full p-[2px] outline-none transition-[background-color] duration-200 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-55 {checked
    ? 'bg-primary'
    : 'bg-input'}"
  onclick={() => onchange(!checked)}
  {...rest}
>
  <span
    class="pointer-events-none block size-[14px] rounded-full bg-white shadow-sm transition-[translate,scale] duration-150 ease-out-ui group-active/switch:scale-x-115 {checked
      ? 'origin-right translate-x-[12px]'
      : 'origin-left translate-x-0 dark:bg-neutral-300'}"
  ></span>
</button>
