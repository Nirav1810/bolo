<script lang="ts">
  import { BookA, Plus, Upload } from "@lucide/svelte";
  import { plural, type Form, type SettingsView } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import Chip from "../lib/ui/Chip.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Status from "../lib/ui/Status.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let word = $state("");
  let note = $state<{ tone: "success" | "warning"; text: string } | null>(null);
  let fileInput: HTMLInputElement | undefined = $state();
  let wordInput: HTMLInputElement | undefined = $state();

  /** Words from a pasted or imported list: one per line, or separated by commas. */
  function split(text: string): string[] {
    return text
      .split(/[\n,;]+/)
      .map((w) => w.trim())
      .filter(Boolean);
  }

  function add(words: string[]) {
    if (!words.length) return;
    const before = view.form.dictionary.length;
    update((f) => (f.dictionary = [...f.dictionary, ...words])).then(() => {
      const added = view.form.dictionary.length - before;
      if (words.length === 1) note = added ? null : { tone: "warning", text: `“${words[0]}” is already in the dictionary.` };
      else if (added === words.length) note = { tone: "success", text: `Added ${plural(added, "word", "words")}.` };
      else note = { tone: "success", text: `Added ${added} of ${words.length}. The rest were already there.` };
    });
  }

  function remove(term: string) {
    note = null;
    update((f) => (f.dictionary = f.dictionary.filter((t) => t !== term)));
    // The chip under the keyboard's focus is gone; the field is the next thing to use.
    wordInput?.focus();
  }

  async function importFile(e: Event & { currentTarget: HTMLInputElement }) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = "";
    if (file) add(split(await file.text()));
  }
</script>

<SettingsSection title="Dictionary" description="Used by the speech engine and by polish.">
  {#snippet action()}
    <span class="px-1 text-xs text-muted-foreground tabular-nums">{plural(view.form.dictionary.length, "word", "words")}</span>
    <Button variant="ghost" size="sm" onclick={() => fileInput?.click()}><Upload aria-hidden="true" />Import…</Button>
    <input bind:this={fileInput} type="file" accept=".txt,.csv,text/plain" class="sr-only" tabindex="-1" aria-hidden="true" onchange={importFile} />
  {/snippet}
  <form
    class="flex flex-col gap-1.5 px-4 py-3"
    onsubmit={(e) => {
      e.preventDefault();
      add(split(word));
      word = "";
    }}
  >
    <div class="flex items-center gap-2">
      <label for="word" class="sr-only">Word or name</label>
      <input bind:this={wordInput} id="word" class="field field-sm" placeholder="Word or name, like Priya or Kubernetes…" autocomplete="off" bind:value={word} />
      <Button type="submit" variant="primary" size="sm" disabled={!word.trim()}><Plus aria-hidden="true" />Add word</Button>
    </div>
    {#if note}<Status tone={note.tone}>{note.text}</Status>{/if}
  </form>

  {#if view.form.dictionary.length}
    <ul class="flex flex-wrap gap-1.5 px-4 py-3.5" aria-label="Dictionary words">
      {#each view.form.dictionary as term (term)}
        <Chip label={term} onremove={() => remove(term)} />
      {/each}
    </ul>
  {:else}
    <Empty icon={BookA} title="No words yet" hint="Add names, brands and jargon. A text file with one word per line imports too." />
  {/if}
</SettingsSection>
