<script lang="ts">
  // Where an API key is pasted, the same on Engines and keys, Usage and onboarding: a
  // masked field and one submit. The page's `onsave` checks the key with its provider and
  // saves it (lib/api.ts `saveCheckedKey`); what it returns is shown under the field.
  import Button from "./Button.svelte";
  import Status from "./Status.svelte";

  let {
    id,
    label,
    placeholder = "Paste API key…",
    submit = "Save",
    size = "sm",
    disabled = false,
    onsave,
    oncancel,
  }: {
    id: string;
    /** The field's name for a screen reader, when no row title is its `<label>`. */
    label?: string;
    placeholder?: string;
    submit?: string;
    /** `sm` in a row or an Inset; `default` on an onboarding card. */
    size?: "sm" | "default";
    disabled?: boolean;
    /** Checks and saves; resolves to what went wrong, or nothing when it is saved. */
    onsave: (key: string) => Promise<string | null | void>;
    oncancel?: () => void;
  } = $props();

  let key = $state("");
  let busy = $state(false);
  let error = $state("");

  async function onsubmit(e: SubmitEvent) {
    e.preventDefault();
    const pasted = key.trim();
    if (!pasted || busy) return;
    busy = true;
    error = "";
    try {
      error = (await onsave(pasted)) ?? "";
    } catch (e) {
      error = String(e);
    }
    busy = false;
    if (!error) key = "";
  }
</script>

<form class="flex min-w-0 flex-col gap-1.5" {onsubmit}>
  <div class="flex items-center gap-1.5">
    {#if label}<label for={id} class="sr-only">{label}</label>{/if}
    <input
      {id}
      class="field font-mono {size === 'sm' ? 'field-sm' : ''}"
      type="password"
      {placeholder}
      autocomplete="off"
      spellcheck="false"
      aria-invalid={error ? "true" : undefined}
      aria-describedby={error ? `${id}-error` : undefined}
      disabled={disabled || busy}
      bind:value={key}
    />
    {#if oncancel}<Button variant="ghost" {size} onclick={oncancel}>Cancel</Button>{/if}
    <Button type="submit" variant="primary" {size} loading={busy} disabled={disabled || !key.trim()}>{submit}</Button>
  </div>
  {#if busy}
    <Status tone="busy">Checking…</Status>
  {:else if error}
    <div id="{id}-error"><Status tone="error">{error}</Status></div>
  {/if}
</form>
