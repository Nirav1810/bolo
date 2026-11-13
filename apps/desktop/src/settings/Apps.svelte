<script lang="ts">
  // Per-app profiles (ADR-0020): each app's format, polish and tone. A field the user
  // sets wins over Bolo's bundled app rules (src-tauri/app_rules.toml), which win over
  // the defaults. Code mode and how text is pasted stay Bolo's.
  import { tick } from "svelte";
  import { AppWindow, ChevronDown, CodeXml, Plus } from "@lucide/svelte";
  import { FORMAT_NAMES, TONE_NAMES, api, sentence, type AppChange, type AppView, type Field as AppField, type Format, type SettingsView } from "../lib/api";
  import { PAGES } from "../lib/pages";
  import Alert from "../lib/ui/Alert.svelte";
  import Badge from "../lib/ui/Badge.svelte";
  import Button from "../lib/ui/Button.svelte";
  import ConfirmButton from "../lib/ui/ConfirmButton.svelte";
  import Empty from "../lib/ui/Empty.svelte";
  import Field from "../lib/ui/Field.svelte";
  import IconTile from "../lib/ui/IconTile.svelte";
  import Inset from "../lib/ui/Inset.svelte";
  import Select from "../lib/ui/Select.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

  let { view = $bindable(), onkeys }: { view: SettingsView; onkeys: () => void } = $props();

  let open = $state<string | null>(null);
  let newApp = $state("");
  let adding = $state(false);
  let error = $state<{ title: string; detail: string } | null>(null);
  let newAppInput: HTMLInputElement | undefined = $state();

  const toneName = (t: string) => TONE_NAMES[t] ?? t;

  const hasKey = $derived(!!view.keys.anthropic);
  const added = $derived(view.apps.filter((a) => !a.bundled));
  const builtIn = $derived(view.apps.filter((a) => a.bundled));

  /** Where a field's value comes from, as a hint under it. */
  function sourceText(f: AppField<unknown>, fallback: string): string {
    if (f.source === "user") return "Set by you.";
    if (f.source === "bundled") return "Set by Bolo for this app.";
    return fallback;
  }

  /** Runs a change and takes the view Rust returns; resolves to whether it worked. */
  async function run(title: string, call: () => Promise<SettingsView>): Promise<boolean> {
    try {
      view = await call();
      error = null;
      return true;
    } catch (e) {
      error = { title, detail: sentence(String(e)) };
      return false;
    }
  }

  const set = (a: AppView, change: AppChange) => run(`Couldn’t save ${a.name.value}`, () => api.appSet(a.exe, change));
  const reset = (a: AppView) => run(`Couldn’t save ${a.name.value}`, () => api.appReset(a.exe));

  async function remove(a: AppView) {
    if (await run(`Couldn’t delete ${a.name.value}`, () => api.appReset(a.exe))) {
      if (open === a.exe) open = null;
      // The row under the keyboard's focus is gone; the add field is the next thing to use.
      newAppInput?.focus();
    }
  }

  async function add(exe: string) {
    adding = true;
    const ok = await run(`Couldn’t add ${exe.trim()}`, () => api.appAdd(exe));
    adding = false;
    if (!ok) return;
    newApp = "";
    const row = view.apps.find((a) => a.exes.includes(exe.trim().toLowerCase().replace(/(\.exe)?$/, ".exe")));
    if (row) open = row.exe;
    await tick();
    newAppInput?.focus();
  }

  /** A Select's value: "" for "Default" (no choice of the user's). */
  const chosen = (f: AppField<string>) => (f.source === "user" ? f.value : "");
</script>

{#if error}
  <Alert variant="error" title={error.title}>{error.detail}</Alert>
{/if}

{#if !hasKey}
  <Alert variant="warning" title="Polish needs an Anthropic API key">
    Per-app polish and tone apply once there is one. Format works without it.
    {#snippet actions()}
      <Button variant="outline" size="sm" onclick={onkeys}>Open {PAGES.engines.label}</Button>
    {/snippet}
  </Alert>
{/if}

{#snippet appRow(a: AppView)}
  {@const expanded = open === a.exe}
  {@const panel = `app-settings-${a.exe}`}
  <SettingsRow title={a.name.value}>
    {#snippet leading()}<IconTile icon={a.code_mode ? CodeXml : AppWindow} />{/snippet}
    {#snippet badges()}
      {#if a.custom}<Badge variant="primary">Custom</Badge>{/if}
      {#if a.code_mode}<Badge variant="neutral">Code</Badge>{/if}
    {/snippet}
    {#snippet description()}
      <span class="font-mono">{a.exes.join(", ")}</span>
      · {FORMAT_NAMES[a.format.value]}{a.polish.value ? ` · ${toneName(a.tone.value)} polish` : ""}
    {/snippet}
    {#snippet control()}
      <Switch aria-label="Polish in {a.name.value}" checked={a.polish.value} onchange={(on) => set(a, { field: "polish", value: on })} />
      {#if !a.bundled}
        <ConfirmButton label="Delete {a.name.value}" onconfirm={() => remove(a)} />
      {/if}
      <Button
        variant="quiet"
        size="icon-sm"
        aria-expanded={expanded}
        aria-controls={panel}
        aria-label="{expanded ? 'Hide' : 'Show'} settings for {a.name.value}"
        onclick={() => (open = expanded ? null : a.exe)}
      >
        <ChevronDown class="transition-transform {expanded ? 'rotate-180' : ''}" aria-hidden="true" />
      </Button>
    {/snippet}
  </SettingsRow>
  {#if expanded}
    <Inset id={panel} aria-label="Settings for {a.name.value}">
      <div class="grid grid-cols-2 gap-3">
        <Field label="Format" for="{panel}-format" hint={sourceText(a.format, "Follows Bolo’s default.")}>
          <Select
            id="{panel}-format"
            class="w-full"
            value={chosen(a.format)}
            options={[["", `Default · ${FORMAT_NAMES[a.inherited.format.value]}`], ...Object.entries(FORMAT_NAMES)]}
            onchange={(v) => set(a, { field: "format", value: (v || null) as Format | null })}
          />
        </Field>
        <Field label="Polish" for="{panel}-polish" hint={sourceText(a.polish, `Follows the switch on ${PAGES.polish.label}.`)}>
          <Select
            id="{panel}-polish"
            class="w-full"
            value={a.polish.source === "user" ? String(a.polish.value) : ""}
            options={[
              ["", `Default · ${a.inherited.polish.value ? "On" : "Off"}`],
              ["true", "On"],
              ["false", "Off"],
            ]}
            onchange={(v) => set(a, { field: "polish", value: v === "" ? null : v === "true" })}
          />
        </Field>
        <Field label="Tone" for="{panel}-tone" hint={sourceText(a.tone, `Follows the tone on ${PAGES.polish.label}.`)}>
          <Select
            id="{panel}-tone"
            class="w-full"
            value={chosen(a.tone)}
            options={[["", `Default · ${toneName(a.inherited.tone.value)}`], ...Object.entries(TONE_NAMES)]}
            onchange={(v) => set(a, { field: "tone", value: v || null })}
          />
        </Field>
        <Field label="Name" for="{panel}-name" hint={sourceText(a.name, "Taken from the program name.")}>
          <input
            id="{panel}-name"
            class="field field-sm"
            value={a.name.source === "user" ? a.name.value : ""}
            placeholder={a.inherited.name.value}
            aria-describedby="{panel}-name-hint"
            autocomplete="off"
            spellcheck="false"
            onchange={(e) => set(a, { field: "name", value: e.currentTarget.value.trim() || null })}
          />
        </Field>
      </div>
      {#if a.bundled && a.custom}
        <div class="flex justify-end">
          {#if a.name.source === "user"}
            <!-- Resetting erases the name the user typed, so it asks first. -->
            <ConfirmButton label="Reset {a.name.value}" text="Reset" confirm="Reset?" onconfirm={() => reset(a)} />
          {:else}
            <Button variant="ghost" size="sm" onclick={() => reset(a)}>Reset</Button>
          {/if}
        </div>
      {/if}
    </Inset>
  {/if}
{/snippet}

<SettingsSection title="Your apps" description="Apps you added. Pick a format, polish and tone for each.">
  <form
    class="flex flex-col gap-2 px-4 py-3"
    onsubmit={(e) => {
      e.preventDefault();
      add(newApp);
    }}
  >
    <div class="flex items-center gap-2">
      <label for="new-app" class="sr-only">Program name</label>
      <input
        bind:this={newAppInput}
        id="new-app"
        class="field field-sm font-mono"
        placeholder="Program name, like whatsapp.exe…"
        autocomplete="off"
        spellcheck="false"
        bind:value={newApp}
      />
      <Button type="submit" variant="primary" size="sm" loading={adding} disabled={!newApp.trim()}><Plus aria-hidden="true" />Add app</Button>
    </div>
    {#if view.recent_app}
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-xs text-muted-foreground">Last app you used</span>
        <Button variant="ghost" size="sm" disabled={adding} onclick={() => add(view.recent_app ?? "")}>
          <Plus aria-hidden="true" />Add {view.recent_app}
        </Button>
      </div>
    {/if}
  </form>
  {#each added as a (a.exe)}
    {@render appRow(a)}
  {:else}
    <Empty icon={AppWindow} title="No apps yet" hint="Add an app by its program name to pick its format, polish and tone." />
  {/each}
</SettingsSection>

<SettingsSection title="Built-in apps" description="Bolo’s settings for these apps. Change any of them; your choice wins.">
  {#each builtIn as a (a.exe)}
    {@render appRow(a)}
  {/each}
</SettingsSection>
