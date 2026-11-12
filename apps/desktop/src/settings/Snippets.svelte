<script lang="ts">
  // Snippets, on the Dictionary & snippets tab: a trigger phrase and the text Bolo types
  // in its place. One editor at a time, for a new snippet or an existing one; a change is
  // saved when the editor is saved (the form is tidied in Rust, src-tauri/src/form.rs).
  import { MessageSquareQuote, Pencil, Plus, Trash2 } from "@lucide/svelte";
  import type { Form, SettingsView, TextSnippet } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import Empty from "../lib/ui/Empty.svelte";
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

  function open(which: number | "new") {
    editing = which;
    draft = which === "new" ? { trigger: "", text: "" } : { ...snippets[which] };
    queueMicrotask(() => triggerInput?.focus());
  }

  function close() {
    editing = null;
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

  function remove(index: number) {
    if (editing === index) close();
    else if (typeof editing === "number" && editing > index) editing -= 1;
    update((f) => (f.snippets = f.snippets.filter((_, i) => i !== index)));
  }
</script>

{#snippet editor(label: string)}
  <form
    class="flex flex-col gap-3 bg-muted/40 px-4 py-3.5"
    aria-label={label}
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <div class="flex flex-col gap-1.5">
      <div class="flex items-baseline justify-between gap-3">
        <label for="snippet-trigger" class="text-xs font-medium text-foreground">When you say</label>
        <span id="snippet-trigger-hint" class="text-2xs text-muted-foreground">Case and punctuation don’t matter</span>
      </div>
      <input
        bind:this={triggerInput}
        id="snippet-trigger"
        class="input"
        placeholder="A phrase you wouldn’t say otherwise, like “my home address”"
        aria-describedby="snippet-trigger-hint"
        autocomplete="off"
        spellcheck="false"
        bind:value={draft.trigger}
        onkeydown={keys}
      />
    </div>
    <label class="flex flex-col gap-1.5">
      <span class="text-xs font-medium text-foreground">Bolo types</span>
      <textarea
        class="textarea text-[13px]"
        rows="4"
        placeholder={"221B Baker Street\nLondon NW1 6XE"}
        spellcheck="false"
        bind:value={draft.text}
        onkeydown={keys}
      ></textarea>
    </label>
    <div class="flex items-center justify-between gap-3">
      {#if problem}
        <p class="text-xs text-destructive-foreground" role="alert">{problem}</p>
      {:else}
        <p class="text-xs text-muted-foreground">Typed exactly as written, line breaks included. Polish leaves it alone.</p>
      {/if}
      <div class="flex shrink-0 items-center gap-1.5">
        <Button variant="ghost" size="sm" onclick={close}>Cancel</Button>
        <Button type="submit" variant="primary" size="sm" disabled={!ready}>{editing === "new" ? "Add snippet" : "Save"}</Button>
      </div>
    </div>
  </form>
{/snippet}

<SettingsSection title="Snippets" description="Say a trigger and Bolo types the saved text in its place.">
  {#snippet action()}
    <span class="text-xs text-muted-foreground tabular-nums">{snippets.length} {snippets.length === 1 ? "snippet" : "snippets"}</span>
    {#if snippets.length}
      <Button variant="ghost" size="sm" disabled={editing === "new"} onclick={() => open("new")}><Plus aria-hidden="true" />New snippet</Button>
    {/if}
  {/snippet}

  {#if editing === "new"}
    {@render editor("New snippet")}
  {/if}

  {#if snippets.length}
    <ul class="divide-y divide-border/60" aria-label="Snippets">
      {#each snippets as s, i (s.trigger)}
        <li>
          {#if editing === i}
            {@render editor(`Edit “${s.trigger}”`)}
          {:else}
            <div class="group/row flex items-start gap-3 px-4 py-3">
              <div class="grid size-8 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-xs/5 dark:bg-input/24">
                <MessageSquareQuote class="size-4 text-muted-foreground" aria-hidden="true" />
              </div>
              <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                <p class="truncate text-sm font-medium text-foreground">“{s.trigger}”</p>
                <p class="line-clamp-2 text-xs whitespace-pre-line text-muted-foreground">{s.text}</p>
              </div>
              <div class="flex shrink-0 items-center gap-0.5">
                <Button variant="ghost-muted" size="icon-sm" aria-label="Edit {s.trigger}" title="Edit" onclick={() => open(i)}>
                  <Pencil aria-hidden="true" />
                </Button>
                <Button variant="ghost-destructive" size="icon-sm" aria-label="Remove {s.trigger}" title="Remove" onclick={() => remove(i)}>
                  <Trash2 aria-hidden="true" />
                </Button>
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if editing !== "new"}
    <Empty icon={MessageSquareQuote} title="No snippets yet" hint="Save an address, a sign-off or a link, and say a short phrase to type it.">
      <Button variant="outline" size="sm" class="mt-2" onclick={() => open("new")}><Plus aria-hidden="true" />New snippet</Button>
    </Empty>
  {/if}
</SettingsSection>
