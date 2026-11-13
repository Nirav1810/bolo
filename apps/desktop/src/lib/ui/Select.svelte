<script lang="ts">
  // One choice out of a list: a button that shows the choice and opens a list under it.
  // It stands in for the native `<select>`, whose list can't be styled. Keyboard as a
  // native one: Up/Down, Home/End and letters move, Enter or Space chooses, Esc closes.
  import { Check, ChevronsUpDown } from "@lucide/svelte";
  import { cn } from "./cn";

  let {
    id,
    value,
    options,
    onchange,
    onopen,
    label,
    describedby,
    size = "sm",
    disabled = false,
    class: className = "",
  }: {
    /** For the `<label for>` that names it (SettingsRow `labelFor`, Field `for`). */
    id?: string;
    value: string;
    options: readonly (readonly [value: string, label: string])[];
    onchange: (value: string) => void;
    /** Called as the list opens, to refresh what it offers. */
    onopen?: () => void;
    /** Its name, when no `<label>` points at it. */
    label?: string;
    /** The id of the hint under it. */
    describedby?: string;
    /** `sm` (28 px) in a row or an Inset; `default` (32 px) beside a 32 px field. */
    size?: "sm" | "default";
    disabled?: boolean;
    /** Width only: `w-40`, `w-full`, `max-w-64`. */
    class?: string;
  } = $props();

  const uid = $props.id();
  let button: HTMLButtonElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();
  let open = $state(false);
  let active = $state(0);

  const chosen = $derived(options.find(([v]) => v === value));

  function place() {
    if (!button || !list) return;
    const r = button.getBoundingClientRect();
    list.style.minWidth = `${r.width}px`;
    list.style.maxHeight = "";
    const room = innerHeight - r.bottom - 12;
    const above = r.top - 12;
    const height = list.offsetHeight;
    const below = height <= room || room >= above;
    list.style.maxHeight = `${Math.max(120, Math.min(320, below ? room : above))}px`;
    list.style.left = `${Math.max(8, Math.min(r.left, innerWidth - list.offsetWidth - 8))}px`;
    list.style.top = below ? `${r.bottom + 4}px` : `${Math.max(8, r.top - 4 - list.offsetHeight)}px`;
  }

  function ontoggle(e: ToggleEvent) {
    open = e.newState === "open";
    if (!open) return;
    onopen?.();
    active = Math.max(
      0,
      options.findIndex(([v]) => v === value),
    );
    place();
    requestAnimationFrame(() => list?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" }));
  }

  // The list is placed once, when it opens; scrolling the page under it closes it.
  $effect(() => {
    if (!open) return;
    const close = (e: Event) => {
      if (e.target instanceof Node && list?.contains(e.target)) return;
      list?.hidePopover();
    };
    addEventListener("scroll", close, true);
    addEventListener("resize", close);
    return () => {
      removeEventListener("scroll", close, true);
      removeEventListener("resize", close);
    };
  });

  // The options can change while the list is open (a microphone is plugged in).
  $effect(() => {
    void options.length;
    if (open) place();
  });

  function choose(i: number) {
    const picked = options[i];
    list?.hidePopover();
    button?.focus();
    if (picked && picked[0] !== value) onchange(picked[0]);
  }

  function move(to: number) {
    active = Math.max(0, Math.min(options.length - 1, to));
    requestAnimationFrame(() => list?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" }));
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        list?.showPopover();
      }
      return;
    }
    if (e.key === "ArrowDown") move(active + 1);
    else if (e.key === "ArrowUp") move(active - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(options.length - 1);
    else if (e.key === "Enter" || e.key === " ") choose(active);
    else if (e.key === "Tab") {
      list?.hidePopover();
      return;
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      const from = options.findIndex(([, text], i) => i > active && text.toLowerCase().startsWith(e.key.toLowerCase()));
      const first = options.findIndex(([, text]) => text.toLowerCase().startsWith(e.key.toLowerCase()));
      if (from >= 0 || first >= 0) move(from >= 0 ? from : first);
      return;
    } else return;
    e.preventDefault();
  }
</script>

<button
  bind:this={button}
  {id}
  type="button"
  role="combobox"
  class={cn("select-trigger field", size === "sm" && "field-sm", className)}
  aria-label={label}
  aria-describedby={describedby}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls="{uid}-list"
  aria-activedescendant={open ? `${uid}-${active}` : undefined}
  popovertarget="{uid}-list"
  {disabled}
  {onkeydown}
>
  <span class="min-w-0 flex-1 truncate text-left">{chosen?.[1] ?? value}</span>
  <ChevronsUpDown class="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
</button>
<div bind:this={list} id="{uid}-list" popover="auto" role="listbox" tabindex="-1" class="select-list" {ontoggle}>
  {#each options as [v, text], i (v)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      id="{uid}-{i}"
      role="option"
      tabindex="-1"
      aria-selected={v === value}
      data-active={i === active}
      class="select-option"
      onpointermove={() => (active = i)}
      onclick={() => choose(i)}
    >
      <span class="min-w-0 flex-1 truncate">{text}</span>
      {#if v === value}<Check class="size-3.5 shrink-0 text-primary-text" aria-hidden="true" />{/if}
    </div>
  {/each}
</div>
