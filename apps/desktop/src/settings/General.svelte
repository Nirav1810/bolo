<script lang="ts">
  import { Keyboard } from "@lucide/svelte";
  import { api, keyFromCode, keyLabel, type Form, type SettingsView } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";
  import Kbd from "../lib/ui/Kbd.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let capturing = $state(false);
  let captureNote = $state("");

  async function startCapture() {
    captureNote = "";
    capturing = true;
    // Bolo lets go of its key while the window listens for the new one.
    await api.capture(true);
  }

  async function endCapture() {
    capturing = false;
    await api.capture(false);
  }

  async function onKey(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    if (e.code === "Escape") {
      await endCapture();
      return;
    }
    const key = keyFromCode(e.code);
    if (!key) {
      captureNote = "That key can't be used. Press Caps Lock, Right Ctrl, Right Alt or F13–F24.";
      return;
    }
    capturing = false;
    await update((f) => (f.push_to_talk = key));
    await api.capture(false);
  }

  $effect(() => {
    // Never leave Bolo without its key if the page goes away mid-capture.
    return () => {
      if (capturing) api.capture(false);
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<SettingsSection title="Basics">
  <SettingsRow title="Push-to-talk key" titleId="ptt">
    {#snippet description()}
      {#if capturing}
        Press the key you want, or <Kbd>Esc</Kbd> to keep {keyLabel(view.form.push_to_talk)}.
      {:else if view.form.push_to_talk === "caps-lock"}
        Hold to talk, release to insert. A quick tap still toggles Caps Lock. <Kbd>Esc</Kbd> cancels.
      {:else}
        Hold to talk, release to insert. <Kbd>Esc</Kbd> cancels.
      {/if}
    {/snippet}
    {#snippet status()}
      {#if captureNote}<span class="text-warning-foreground" role="status">{captureNote}</span>{/if}
    {/snippet}
    {#snippet control()}
      {#if capturing}
        <Button variant="ghost-muted" size="sm" onclick={endCapture}>Cancel</Button>
      {/if}
      <Button
        variant={capturing ? "primary" : "outline"}
        class="min-w-32 justify-between"
        aria-describedby="ptt"
        onclick={startCapture}
      >
        <Keyboard aria-hidden="true" />
        <span class="flex-1 text-center">{capturing ? "Press a key…" : keyLabel(view.form.push_to_talk)}</span>
      </Button>
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Start with Windows" labelFor="autostart" description="Bolo waits in the tray, ready for your key.">
    {#snippet control()}
      <Switch id="autostart" checked={view.form.autostart} onchange={(on) => update((f) => (f.autostart = on))} />
    {/snippet}
  </SettingsRow>
</SettingsSection>

<SettingsSection title="Language">
  <SettingsRow title="Language mode" description="Auto listens for English and Hinglish together and keeps the better one.">
    {#snippet control()}
      <Segmented
        label="Language mode"
        value={view.form.language}
        options={[
          ["auto", "Auto"],
          ["english", "English"],
          ["hinglish", "Hinglish"],
        ]}
        onchange={(v) => update((f) => (f.language = v as Form["language"]))}
      />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Hinglish script" description="How Hindi words are written.">
    {#snippet control()}
      <Segmented
        label="Hinglish script"
        value={view.form.script}
        options={[
          ["roman", "Roman"],
          ["devanagari", "देवनागरी"],
        ]}
        onchange={(v) => update((f) => (f.script = v as Form["script"]))}
      />
    {/snippet}
  </SettingsRow>
</SettingsSection>
