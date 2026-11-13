<script lang="ts">
  // Snippets, on the Dictionary and snippets page: a trigger phrase and the text Bolo
  // inserts in its place. One editor at a time, for a new snippet or an existing one; a
  // change is saved when the editor is saved (the form is tidied in Rust,
  // src-tauri/src/form.rs).
  import { tick } from "svelte";
  import { MessageSquareQuote, Pencil, Plus } from "@lucide/svelte";
  import { plural, type Form, type SettingsView, type TextSnippet } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import Field from "../lib/ui/Field.svelte";
  import IconTile from "../lib/ui/IconTile.svelte";
  import Inset from "../lib/ui/Inset.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  /** The snippet being edited: its index, or "new"; `null` when no editor is open. */
  let editing = $state<number | "new" | null>(null);
  let draft = $state<TextSnippet>({ trigger: "", text: "" });
  let triggerInput: HTMLInputElement | undefined = $state();

  const snippets = $derived(view.form.snippets);

  /** Why the draft can't be saved yet, or "" when it can. */
  const problem = $derived.by(() => {
    const trigger = draft.trigger.trim().toLowerCase();
    if (!trigger || !draft.text.trim()) return "";
    const taken = snippets.findIndex((s, i) => i !== editing && s.trigger.trim().toLowerCase() === trigger);
    return taken >= 0 ? `You already have a snippet for “${snippets[taken].trigger}”.` : "";
  });
  const ready = $derived(!!draft.trigger.trim() && !!draft.text.trim() && !problem);

  /** The button that opens the editor for `which`: focus goes back to it on close. */
  function opener(which: number | "new"): HTMLElement | null {
    return document.querySelector<HTMLElement>(which === "new" ? "[data-snippet-new]" : `[data-snippet-edit="${which}"]`);
  }

  async function open(which: number | "new") {
    editing = which;
    draft = which === "new" ? { trigger: "", text: "" } : { ...snippets[which] };
    await tick();
    triggerInput?.focus();
  }

  async function close() {
    const was = editing;
    editing = null;
    if (was === null) return;
    await tick();
    opener(was)?.focus();
  }

  function save() {
    if (!ready || editing === null) return;
    const snippet = { trigger: draft.trigger.trim(), text: draft.text };
    const at = editing;
    update((f) => {
      if (at === "new") f.snippets = [...f.snippets, snippet];
      else f.snippets = f.snippets.map((s, i) => (i === at ? snippet : s));
    }).then(close);
  }

  /** Escape cancels; Ctrl+Enter saves, also from the text box. */
  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save();
    }
  }

  async function remove(index: number) {
    if (editing === index) editing = null;
    else if (typeof editing === "number" && editing > index) editing -= 1;
    await update((f) => (f.snippets = f.snippets.filter((_, i) => i !== index)));
    // The row under the keyboard's focus is gone: go to the next one, else the one
    // before, else the button that makes a new one.
    await tick();
    const next = Math.min(index, snippets.length - 1);
    (next >= 0 ? opener(next) : opener("new"))?.focus();
  }
</script>

{#snippet editor(label: string)}
  <li>
    <Inset
      as="form"
      id="snippet-editor"
      aria-label={label}
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <Field label="When you say" for="snippet-trigger" hint="Case and punctuation don’t matter." error={problem || undefined}>
        <input
          bind:this={triggerInput}
          id="snippet-trigger"
          class="field field-sm"
          placeholder="Phrase you wouldn’t say otherwise, like “my home address”…"
          aria-describedby={problem ? undefined : "snippet-trigger-hint"}
          aria-invalid={problem ? "true" : undefined}
          autocomplete="off"
          spellcheck="false"
          bind:value={draft.trigger}
          onkeydown={keys}
        />
      </Field>
      <Field label="Bolo inserts" for="snippet-text" hint="Inserted exactly as written, line breaks included. Polish leaves it alone.">
        <textarea
          id="snippet-text"
          class="field"
          rows="4"
          placeholder="Saved text, like 221B Baker Street, London NW1 6XE…"
          aria-describedby="snippet-text-hint"
          spellcheck="false"
          bind:value={draft.text}
          onkeydown={keys}
        ></textarea>
      </Field>
      <div class="flex justify-end gap-1.5">
        <Button variant="ghost" size="sm" onclick={close}>Cancel</Button>
        <Button type="submit" variant="primary" size="sm" disabled={!ready}>{editing === "new" ? "Add snippet" : "Save"}</Button>
      </div>
    </Inset>
  </li>
{/snippet}

<SettingsSection title="Snippets" description="Say a trigger and Bolo inserts the saved text in its place." as="ul">
  {#snippet action()}
    <span class="px-1 text-xs text-muted-foreground tabular-nums">{plural(snippets.length, "snippet", "snippets")}</span>
    <Button
      variant="ghost"
      size="sm"
      data-snippet-new
      aria-expanded={editing === "new"}
      aria-controls={editing === "new" ? "snippet-editor" : undefined}
      disabled={editing === "new"}
      onclick={() => open("new")}><Plus aria-hidden="true" />New snippet…</Button
    >
  {/snippet}

  {#if editing === "new"}
    {@render editor("New snippet")}
  {/if}

  {#each snippets as s, i (s.trigger)}
    <SettingsRow as="li" align="start" title="“{s.trigger}”">
      {#snippet leading()}<IconTile icon={MessageSquareQuote} />{/snippet}
      {#snippet description()}<span class="line-clamp-2 whitespace-pre-line">{s.text}</span>{/snippet}
      {#snippet control()}
        <Button
          variant="quiet"
          size="icon-sm"
          data-snippet-edit={i}
          aria-label="Edit “{s.trigger}”"
          aria-expanded={editing === i}
          aria-controls={editing === i ? "snippet-editor" : undefined}
          onclick={() => (editing === i ? close() : open(i))}
        >
          <Pencil aria-hidden="true" />
        </Button>
        <ConfirmButton label="Delete “{s.trigger}”" onconfirm={() => remove(i)} />
      {/snippet}
    </SettingsRow>
    {#if editing === i}
      {@render editor(`Edit “${s.trigger}”`)}
    {/if}
  {:else}
    {#if editing !== "new"}
      <li>
        <Empty icon={MessageSquareQuote} title="No snippets yet" hint="Save an address, a sign-off or a link, and say a short phrase to insert it." />
      </li>
    {/if}
  {/each}
</SettingsSection>
