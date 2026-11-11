<script lang="ts">
  import type { Form, SettingsView } from "../lib/api";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let word = $state("");
  let imported = $state("");

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
      imported = words.length > 1 ? `Added ${added} of ${words.length}; the rest were already there.` : "";
      if (added === words.length && words.length > 1) imported = `Added ${added} words.`;
    });
  }

  async function importFile(e: Event & { currentTarget: HTMLInputElement }) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = "";
    if (file) add(split(await file.text()));
  }
</script>

<p class="mb-2 text-text-2">Names and terms Bolo should spell your way. Used by the speech engine and by polish.</p>
<form
  class="flex gap-2"
  onsubmit={(e) => {
    e.preventDefault();
    add(split(word));
    word = "";
  }}
>
  <label for="word" class="sr-only">New word or name</label>
  <input id="word" class="input flex-1" placeholder="Add a word or name" bind:value={word} />
  <button type="submit" class="btn primary" disabled={!word.trim()}>Add</button>
  <label class="btn inline-flex items-center">
    Import list…
    <input type="file" accept=".txt,.csv,text/plain" class="sr-only" onchange={importFile} />
  </label>
</form>
{#if imported}<p class="caption mt-1.5" role="status">{imported}</p>{/if}

{#if view.form.dictionary.length}
  <ul class="mt-3 flex flex-wrap gap-2" aria-label="Dictionary">
    {#each view.form.dictionary as term (term)}
      <li class="chip">
        {term}
        <button
          type="button"
          aria-label="Remove {term}"
          onclick={() => update((f) => (f.dictionary = f.dictionary.filter((t) => t !== term)))}>✕</button
        >
      </li>
    {/each}
  </ul>
{:else}
  <p class="caption mt-3">No words yet. A text file with one word per line imports too.</p>
{/if}
