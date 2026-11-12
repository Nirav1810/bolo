<script lang="ts">
  // Per-app profiles (ADR-0020): each app's format, polish and tone. A field the user
  // sets wins over Bolo's bundled app rules (src-tauri/app_rules.toml), which win over
  // the defaults. Code mode and how text is pasted stay Bolo's.
  import { AppWindow, ChevronDown, CodeXml, Plus, RotateCcw, Trash2 } from "@lucide/svelte";
  import { api, type AppChange, type AppView, type Field, type Format, type SettingsView } from "../lib/api";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

  let { view = $bindable(), onkeys }: { view: SettingsView; onkeys: () => void } = $props();

  let open = $state<string | null>(null);
  let newApp = $state("");
  let error = $state("");

  const FORMATS: Record<Format, string> = { plain: "Plain text", markdown: "Markdown", html: "Rich text" };
  const TONES: Record<string, string> = { neutral: "Neutral", casual: "Casual", formal: "Formal" };
  const toneName = (t: string) => TONES[t] ?? t;

  const hasKey = $derived(!!view.keys.anthropic);
  const added = $derived(view.apps.filter((a) => !a.bundled));
  const builtIn = $derived(view.apps.filter((a) => a.bundled));

  /** Where a field's value comes from, in a few words. */
  function sourceText(f: Field<unknown>, fallback: string): string {
    if (f.source === "user") return "Your choice";
    if (f.source === "bundled") return "Bolo’s setting for this app";
    return fallback;
  }

  async function run(call: () => Promise<SettingsView>) {
    try {
      view = await call();
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  const set = (a: AppView, change: AppChange) => run(() => api.appSet(a.exe, change));
  const reset = (a: AppView) => run(() => api.appReset(a.exe));

  async function add(exe: string) {
    await run(() => api.appAdd(exe));
    if (!error) {
      newApp = "";
      const row = view.apps.find((a) => a.exes.includes(exe.trim().toLowerCase().replace(/(\.exe)?$/, ".exe")));
      if (row) open = row.exe;
    }
  }

  /** A select's value: "" for "Auto" (no choice of the user's). */
  const chosen = (f: Field<string>) => (f.source === "user" ? f.value : "");
</script>

{#if !hasKey}
  <Alert variant="warning" title="Polish needs an Anthropic key">
    Per-app polish and tone apply once there is one. Format works without it.
    {#snippet actions()}
      <Button variant="outline" size="sm" onclick={onkeys}>Add a key</Button>
    {/snippet}
  </Alert>
{/if}

{#if error}
  <Alert variant="error">{error}</Alert>
{/if}

{#snippet appRow(a: AppView)}
  {@const expanded = open === a.exe}
  <SettingsRow title={a.name.value}>
    {#snippet leading()}
      <div class="grid size-8 place-items-center rounded-lg border border-border/70 bg-muted">
        {#if a.code_mode}
          <CodeXml class="size-4 text-muted-foreground" aria-hidden="true" />
        {:else}
          <AppWindow class="size-4 text-muted-foreground" aria-hidden="true" />
        {/if}
      </div>
    {/snippet}
    {#snippet badges()}
      {#if a.custom}<Badge variant="primary">Custom</Badge>{/if}
      {#if a.code_mode}<Badge>Code</Badge>{/if}
    {/snippet}
    {#snippet description()}
      <span class="mono text-2xs">{a.exes.join(", ")}</span>
      · <span class={a.format.source === "user" ? "text-foreground" : ""}>{FORMATS[a.format.value]}</span>
      {#if a.polish.value}
        · <span class={a.tone.source === "user" ? "text-foreground" : ""}>{toneName(a.tone.value)} polish</span>
      {/if}
    {/snippet}
    {#snippet control()}
      <Switch aria-label="Polish in {a.name.value}" checked={a.polish.value} onchange={(on) => set(a, { field: "polish", value: on })} />
      <Button
        variant="ghost-muted"
        size="icon-xs"
        aria-expanded={expanded}
        aria-controls="app-{a.exe}"
        aria-label="{expanded ? 'Hide' : 'Show'} {a.name.value}’s settings"
        title={expanded ? "Hide settings" : "More settings"}
        onclick={() => (open = expanded ? null : a.exe)}
      >
        <ChevronDown class="transition-transform duration-150 {expanded ? 'rotate-180' : ''}" aria-hidden="true" />
      </Button>
    {/snippet}
    {#if expanded}
      <div id="app-{a.exe}" class="ml-11 flex flex-col gap-3 rounded-lg border border-border/70 bg-muted/40 p-3">
        <div class="grid grid-cols-2 gap-x-4 gap-y-3">
          <label class="flex flex-col gap-1">
            <span class="text-xs font-medium text-foreground">Format</span>
            <select
              class="select h-7! w-full! text-[13px]!"
              value={chosen(a.format)}
              onchange={(e) => set(a, { field: "format", value: (e.currentTarget.value || null) as Format | null })}
            >
              <option value="">Auto · {FORMATS[a.inherited.format.value]}</option>
              {#each Object.entries(FORMATS) as [v, text] (v)}<option value={v}>{text}</option>{/each}
            </select>
            <span class="text-2xs text-muted-foreground">{sourceText(a.format, "Bolo’s default")}</span>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs font-medium text-foreground">Polish</span>
            <select
              class="select h-7! w-full! text-[13px]!"
              value={a.polish.source === "user" ? String(a.polish.value) : ""}
              onchange={(e) => {
                const v = e.currentTarget.value;
                set(a, { field: "polish", value: v === "" ? null : v === "true" });
              }}
            >
              <option value="">Auto · {a.inherited.polish.value ? "On" : "Off"}</option>
              <option value="true">On</option>
              <option value="false">Off</option>
            </select>
            <span class="text-2xs text-muted-foreground">{sourceText(a.polish, "Follows the Polish switch")}</span>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs font-medium text-foreground">Tone</span>
            <select
              class="select h-7! w-full! text-[13px]!"
              value={chosen(a.tone)}
              onchange={(e) => set(a, { field: "tone", value: e.currentTarget.value || null })}
            >
              <option value="">Auto · {toneName(a.inherited.tone.value)}</option>
              {#each Object.entries(TONES) as [v, text] (v)}<option value={v}>{text}</option>{/each}
            </select>
            <span class="text-2xs text-muted-foreground">{sourceText(a.tone, "Follows Polish’s tone")}</span>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs font-medium text-foreground">Name</span>
            <input
              class="input h-7! text-[13px]!"
              value={a.name.source === "user" ? a.name.value : ""}
              placeholder={a.inherited.name.value}
              autocomplete="off"
              spellcheck="false"
              onchange={(e) => set(a, { field: "name", value: e.currentTarget.value.trim() || null })}
            />
            <span class="text-2xs text-muted-foreground">{sourceText(a.name, "From the program name")}</span>
          </label>
        </div>
        {#if a.custom || !a.bundled}
          <div class="flex justify-end border-t border-border/60 pt-2.5">
            {#if a.bundled}
              <Button variant="ghost" size="xs" onclick={() => reset(a)}><RotateCcw aria-hidden="true" />Reset to Bolo’s settings</Button>
            {:else}
              <Button variant="ghost-destructive" size="xs" onclick={() => reset(a)}><Trash2 aria-hidden="true" />Remove {a.name.value}</Button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </SettingsRow>
{/snippet}

<SettingsSection title="Your apps" description="Apps you added. Pick a format, polish and tone for each.">
  {#each added as a (a.exe)}
    {@render appRow(a)}
  {/each}
  <form
    class="flex flex-col gap-2 bg-muted/50 px-4 py-3"
    onsubmit={(e) => {
      e.preventDefault();
      add(newApp);
    }}
  >
    <div class="flex items-center gap-2">
      <label for="new-app" class="sr-only">App to add, by its program name</label>
      <input id="new-app" class="input mono h-7! text-xs!" placeholder="Program name, like whatsapp.exe…" autocomplete="off" spellcheck="false" bind:value={newApp} />
      <Button type="submit" variant="outline" size="sm" disabled={!newApp.trim()}><Plus aria-hidden="true" />Add app</Button>
    </div>
    {#if view.recent_app}
      <p class="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
        Last app you used:
        <Button variant="outline" size="xs" class="mono" onclick={() => add(view.recent_app ?? "")}>
          <Plus aria-hidden="true" />{view.recent_app}
        </Button>
      </p>
    {/if}
  </form>
</SettingsSection>

<SettingsSection title="Built in" description="Bolo’s settings for these apps. Change any of them; your choice wins.">
  {#each builtIn as a (a.exe)}
    {@render appRow(a)}
  {/each}
</SettingsSection>
