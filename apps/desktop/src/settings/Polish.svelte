<script lang="ts">
  import { TONE_NAMES, type Form, type SettingsView } from "../lib/api";
  import { PAGES } from "../lib/pages";
  import Alert from "../lib/ui/Alert.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";
  import TextLink from "../lib/ui/TextLink.svelte";

  let {
    view,
    update,
    onkeys,
    onapps,
  }: {
    view: SettingsView;
    update: (change: (f: Form) => void) => Promise<void>;
    onkeys: () => void;
    onapps: () => void;
  } = $props();

  const hasKey = $derived(view.keys.anthropic !== null && view.keys.anthropic !== undefined);
</script>

{#if !hasKey}
  <Alert variant="warning" title="Polish needs an Anthropic API key">
    Until there is one, Bolo inserts the instant text only.
    {#snippet actions()}
      <Button variant="outline" size="sm" onclick={onkeys}>Open {PAGES.engines.label}</Button>
    {/snippet}
  </Alert>
{/if}

<SettingsSection title="Basics">
  <SettingsRow
    title="Polish with Claude"
    labelFor="polish-on"
    description="Your words are inserted at once. Then Claude tidies them and, if you haven’t typed or clicked since, swaps the polished text in."
  >
    {#snippet control()}
      <Switch id="polish-on" checked={view.form.polish} onchange={(on) => update((f) => (f.polish = on))} />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Tone" description="The voice polish writes in.">
    {#snippet control()}
      <Segmented label="Tone" value={view.form.tone} options={Object.entries(TONE_NAMES)} onchange={(v) => update((f) => (f.tone = v))} />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Per app" description="Turn polish on or off, or pick a tone, for one app. Code editors and terminals start with it off.">
    {#snippet control()}
      <Button variant="outline" size="sm" onclick={onapps}>Open {PAGES.apps.label}</Button>
    {/snippet}
  </SettingsRow>
  {#snippet footer()}
    <Alert variant="info">
      Polish sends the text of each dictation (never the audio) to Anthropic, using your own API key.
      <TextLink href="https://www.anthropic.com/legal/privacy">Anthropic’s privacy policy</TextLink>
    </Alert>
  {/snippet}
</SettingsSection>
