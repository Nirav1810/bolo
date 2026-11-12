<script lang="ts">
  import { api, type Form, type SettingsView } from "../lib/api";
  import Alert from "../lib/ui/Alert.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

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
  <SettingsRow title="Per app" description="Turn polish on or off, or pick a tone, for one app. Code editors and terminals start with it off.">
    {#snippet control()}
      <Button variant="outline" size="sm" onclick={onapps}>Open Apps</Button>
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
