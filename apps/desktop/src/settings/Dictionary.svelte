<script lang="ts">
  import { BookA, Plus, Upload, X } from "@lucide/svelte";
  import type { Form, SettingsView } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let word = $state("");
  let imported = $state("");
  let fileInput: HTMLInputElement | undefined = $state();

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

<SettingsSection
  title="Words"
  description="Used by the speech engine and by polish."
>
  {#snippet action()}
    <span class="text-xs text-muted-foreground tabular-nums">{view.form.dictionary.length} words</span>
    <Button variant="ghost" size="sm" onclick={() => fileInput?.click()}><Upload aria-hidden="true" />Import…</Button>
    <input bind:this={fileInput} type="file" accept=".txt,.csv,text/plain" class="sr-only" tabindex="-1" aria-hidden="true" onchange={importFile} />
  {/snippet}
  <form
    class="flex items-center gap-2 px-4 py-3"
    onsubmit={(e) => {
      e.preventDefault();
      add(split(word));
      word = "";
    }}
  >
    <label for="word" class="sr-only">New word or name</label>
    <input id="word" class="input" placeholder="Add a word or name, like Priya or Kubernetes…" autocomplete="off" bind:value={word} />
    <Button type="submit" variant="primary" disabled={!word.trim()}><Plus aria-hidden="true" />Add</Button>
  </form>
  {#if imported}<p class="px-4 py-2 text-xs text-muted-foreground" role="status">{imported}</p>{/if}

  {#if view.form.dictionary.length}
    <ul class="flex flex-wrap gap-1.5 px-4 py-3.5" aria-label="Dictionary">
      {#each view.form.dictionary as term (term)}
        <li
          class="group/chip inline-flex h-7 items-center gap-1 rounded-lg border border-border bg-background pr-1 pl-2.5 text-[13px] font-medium shadow-xs/5 dark:bg-input/24"
        >
          {term}
          <button
            type="button"
            class="grid size-5 place-items-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            aria-label="Remove {term}"
            onclick={() => update((f) => (f.dictionary = f.dictionary.filter((t) => t !== term)))}
          >
            <X class="size-3" aria-hidden="true" />
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <Empty icon={BookA} title="No words yet" hint="Add names, brands and jargon. A text file with one word per line imports too." />
  {/if}
</SettingsSection>
