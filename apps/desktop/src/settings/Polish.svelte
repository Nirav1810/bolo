<script lang="ts">
  import { api, type AppRow, type Form, type SettingsView } from "../lib/api";
  import Segmented from "../lib/Segmented.svelte";

  let {
    view,
    update,
    onkeys,
  }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void>; onkeys: () => void } = $props();

  let newApp = $state("");

  const hasKey = $derived(view.keys.anthropic !== null && view.keys.anthropic !== undefined);

  const exeOf = (s: string) => {
    const e = s.trim().toLowerCase();
    return e.endsWith(".exe") ? e : `${e}.exe`;
  };

  type Row = { name: string; exe: string; detail: string; fallback: boolean };

  const FORMATS = { plain: "Plain", markdown: "Markdown", html: "HTML" };

  // The apps the bundled rules know, then the user's own.
  const rows = $derived.by(() => {
    const known = view.apps.map(
      (a: AppRow): Row => ({
        name: a.name,
        exe: a.exes[0],
        detail: `${a.exes.join(", ")} · ${a.code_mode ? "Code mode" : FORMATS[a.format]}`,
        fallback: a.polish ?? view.form.polish,
      }),
    );
    const listed = new Set(view.apps.flatMap((a) => a.exes));
    const own = Object.keys(view.form.polish_apps)
      .filter((exe) => !listed.has(exe))
      .map((exe): Row => ({ name: exe.replace(/\.exe$/, ""), exe, detail: `${exe} · Plain`, fallback: view.form.polish }));
    return [...known, ...own];
  });

  function setApp(exe: string, on: boolean | null) {
    update((f) => {
      if (on === null) delete f.polish_apps[exe];
      else f.polish_apps[exe] = on;
    });
  }

  function addApp() {
    const exe = exeOf(newApp);
    if (exe === ".exe") return;
    newApp = "";
    // Added to turn it off, usually; the switch is right there to turn it back on.
    setApp(exe, false);
  }
</script>

<div class="group">
  <div class="card">
    <div class="card-text">
      <label for="polish">Polish with Claude</label>
      <span class="caption">
        Your words go in at once. Then Claude tidies them and, if you haven’t typed or clicked since, the polished text
        replaces them.
      </span>
    </div>
    <input
      id="polish"
      class="switch"
      type="checkbox"
      checked={view.form.polish}
      onchange={(e) => update((f) => (f.polish = e.currentTarget.checked))}
    />
  </div>
  <div class="card">
    <div class="card-text">
      <span>Tone</span>
      <span class="caption">How much polish rewrites.</span>
    </div>
    <Segmented
      label="Tone"
      value={view.form.tone}
      options={[
        ["neutral", "Neutral"],
        ["casual", "Casual"],
        ["formal", "Formal"],
      ]}
      onchange={(v) => update((f) => (f.tone = v))}
    />
  </div>
</div>

<p class="note mt-2">
  <span class="icon" aria-hidden="true">ⓘ</span>
  <span>
    Polish sends the text of each dictation (never the audio) to Anthropic, using your own key.
    <button type="button" class="link" onclick={() => api.openLink("https://www.anthropic.com/legal/privacy")}>
      Anthropic’s privacy policy
    </button>
  </span>
</p>
{#if !hasKey}
  <p class="note warn mt-1.5">
    <span class="icon" aria-hidden="true">⚠</span>
    <span>
      Polish needs an Anthropic key. Until there is one, Bolo inserts the instant text only.
      <button type="button" class="link" onclick={onkeys}>Add a key</button>
    </span>
  </p>
{/if}

<h2 class="section">Per app</h2>
<p class="caption -mt-1 mb-2">Off in code editors and terminals by default. Your choice here wins.</p>
<div class="group">
  {#each rows as r (r.exe)}
    {@const mine = view.form.polish_apps[r.exe]}
    <div class="card compact">
      <div class="card-text">
        <span>{r.name}</span>
        <span class="caption mono">{r.detail}</span>
      </div>
      {#if mine === undefined}
        <span class="tag">Default</span>
      {:else}
        <span class="tag accent">Your choice</span>
        <button type="button" class="link text-caption" onclick={() => setApp(r.exe, null)}>Reset</button>
      {/if}
      <input
        class="switch"
        type="checkbox"
        aria-label="Polish in {r.name}"
        checked={mine ?? r.fallback}
        onchange={(e) => setApp(r.exe, e.currentTarget.checked)}
      />
    </div>
  {/each}
</div>
<form
  class="mt-2 flex gap-2"
  onsubmit={(e) => {
    e.preventDefault();
    addApp();
  }}
>
  <label for="new-app" class="sr-only">App to add, by its program name</label>
  <input id="new-app" class="input w-64" placeholder="Program name, e.g. whatsapp.exe" bind:value={newApp} />
  <button type="submit" class="btn" disabled={!newApp.trim()}>Add app</button>
</form>
