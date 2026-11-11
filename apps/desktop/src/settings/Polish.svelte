<script lang="ts">
  import { AppWindow, Plus, RotateCcw } from "@lucide/svelte";
  import { api, type AppRow, type Form, type SettingsView } from "../lib/api";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

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

  type Row = { name: string; exe: string; exes: string; format: string; fallback: boolean };

  const FORMATS = { plain: "Plain text", markdown: "Markdown", html: "Rich text" };

  // The apps the bundled rules know, then the user's own.
  const rows = $derived.by(() => {
    const known = view.apps.map(
      (a: AppRow): Row => ({
        name: a.name,
        exe: a.exes[0],
        exes: a.exes.join(", "),
        format: a.code_mode ? "Code mode" : FORMATS[a.format],
        fallback: a.polish ?? view.form.polish,
      }),
    );
    const listed = new Set(view.apps.flatMap((a) => a.exes));
    const own = Object.keys(view.form.polish_apps)
      .filter((exe) => !listed.has(exe))
      .map((exe): Row => ({ name: exe.charAt(0).toUpperCase() + exe.slice(1).replace(/\.exe$/, ""), exe, exes: exe, format: FORMATS.plain, fallback: view.form.polish }));
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

<SettingsSection>
  <SettingsRow
    title="Polish with Claude"
    labelFor="polish-on"
    description="Your words go in at once. Then Claude tidies them and, if you haven’t typed or clicked since, the polished text replaces them."
  >
    {#snippet control()}
      <Switch id="polish-on" checked={view.form.polish} onchange={(on) => update((f) => (f.polish = on))} />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Tone" description="How much polish rewrites.">
    {#snippet control()}
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
    {/snippet}
  </SettingsRow>
</SettingsSection>

<div class="-mt-4 flex flex-col gap-2">
  {#if !hasKey}
    <Alert variant="warning" title="Polish needs an Anthropic key">
      Until there is one, Bolo inserts the instant text only.
      {#snippet actions()}
        <Button variant="outline" size="sm" onclick={onkeys}>Add a key</Button>
      {/snippet}
    </Alert>
  {/if}
  <Alert variant="info">
    Polish sends the text of each dictation (never the audio) to Anthropic, using your own key.
    <button
      type="button"
      class="font-medium text-foreground underline decoration-border underline-offset-4 hover:decoration-foreground"
      onclick={() => api.openLink("https://www.anthropic.com/legal/privacy")}>Anthropic’s privacy policy</button
    >
  </Alert>
</div>

<SettingsSection title="Per app" description="Off in code editors and terminals by default. Your choice here wins.">
  {#each rows as r (r.exe)}
    {@const mine = view.form.polish_apps[r.exe]}
    <SettingsRow title={r.name}>
      {#snippet leading()}
        <div class="grid size-8 place-items-center rounded-lg border border-border/70 bg-muted">
          <AppWindow class="size-4 text-muted-foreground" aria-hidden="true" />
        </div>
      {/snippet}
      {#snippet badges()}
        {#if mine !== undefined}<Badge variant="primary">Custom</Badge>{/if}
      {/snippet}
      {#snippet description()}<span class="mono text-2xs">{r.exes}</span> · {r.format}{/snippet}
      {#snippet control()}
        {#if mine !== undefined}
          <Button
            variant="ghost-muted"
            size="icon-xs"
            class="opacity-0 group-hover/row:opacity-100 focus-visible:opacity-100"
            aria-label="Reset {r.name} to the default"
            title="Reset to the default"
            onclick={() => setApp(r.exe, null)}
          >
            <RotateCcw aria-hidden="true" />
          </Button>
        {/if}
        <Switch aria-label="Polish in {r.name}" checked={mine ?? r.fallback} onchange={(on) => setApp(r.exe, on)} />
      {/snippet}
    </SettingsRow>
  {/each}
  <form
    class="flex items-center gap-2 bg-muted/50 px-4 py-3"
    onsubmit={(e) => {
      e.preventDefault();
      addApp();
    }}
  >
    <label for="new-app" class="sr-only">App to add, by its program name</label>
    <input id="new-app" class="input mono h-7! text-xs!" placeholder="Program name, like whatsapp.exe…" autocomplete="off" spellcheck="false" bind:value={newApp} />
    <Button type="submit" variant="outline" size="sm" disabled={!newApp.trim()}><Plus aria-hidden="true" />Add app</Button>
  </form>
</SettingsSection>
