<script lang="ts">
  import { LANGUAGE_NAMES, api, keyFromCode, type Form, type Microphones, type SettingsView } from "../lib/api";
  import Button from "../lib/ui/Button.svelte";
  import Kbd from "../lib/ui/Kbd.svelte";
  import Segmented from "../lib/ui/Segmented.svelte";
  import Select from "../lib/ui/Select.svelte";
  import SettingsRow from "../lib/ui/SettingsRow.svelte";
  import SettingsSection from "../lib/ui/SettingsSection.svelte";
  import Switch from "../lib/ui/Switch.svelte";

  let { view, update }: { view: SettingsView; update: (change: (f: Form) => void) => Promise<void> } = $props();

  let capturing = $state(false);
  let captureNote = $state("");
  let captureButton: HTMLElement | null = null;

  // The microphones plugged in now, read again each time the list opens (hot-plug).
  let mics = $state<Microphones | null>(null);
  const chosenMic = $derived(view.form.input_device);
  const micMissing = $derived(mics !== null && chosenMic !== null && !mics.devices.includes(chosenMic));
  const micOptions = $derived<[string, string][]>([
    ["", "Windows default"],
    ...(mics?.devices ?? []).map((name): [string, string] => [name, name]),
    ...(chosenMic !== null && (mics === null || micMissing) ? [[chosenMic, micMissing ? `${chosenMic} (not plugged in)` : chosenMic] as [string, string]] : []),
  ]);

  async function refreshMics() {
    try {
      mics = await api.microphones();
    } catch {
      // Keep the last list; the select still shows the choice.
    }
  }

  $effect(() => {
    refreshMics();
  });

  async function startCapture(e: MouseEvent) {
    captureButton = e.currentTarget as HTMLElement;
    captureNote = "";
    capturing = true;
    // Bolo lets go of its key while the window listens for the new one.
    await api.capture(true);
  }

  async function endCapture() {
    capturing = false;
    captureButton?.focus();
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
      captureNote = "That key can’t be used. Press Caps Lock, Right Ctrl, Right Alt or F13–F24.";
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
  <SettingsRow title="Push-to-talk key" titleId="ptt-title" status={captureNote ? { tone: "warning", text: captureNote } : null}>
    {#snippet description()}
      {#if capturing}
        Press the key you want, or <Kbd>Esc</Kbd> to keep {view.key_name}.
      {:else if view.form.push_to_talk === "caps-lock"}
        Hold to dictate, let go to insert. A quick tap still toggles Caps Lock. <Kbd>Esc</Kbd> cancels.
      {:else}
        Hold to dictate, let go to insert. <Kbd>Esc</Kbd> cancels.
      {/if}
    {/snippet}
    {#snippet control()}
      {#if capturing}
        <Button variant="ghost" size="sm" onclick={endCapture}>Cancel</Button>
      {/if}
      <Button variant={capturing ? "primary" : "outline"} size="sm" class="min-w-28" aria-describedby="ptt-title" onclick={startCapture}>
        {capturing ? "Press a key…" : view.key_name}
      </Button>
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Microphone" labelFor="microphone">
    {#snippet description()}
      {#if micMissing}
        Not plugged in. Bolo uses the Windows default until it’s back.
      {:else if chosenMic === null && mics?.default}
        Windows default, now {mics.default}.
      {:else}
        The microphone Bolo listens on.
      {/if}
    {/snippet}
    {#snippet control()}
      <Select
        id="microphone"
        class="w-56"
        value={chosenMic ?? ""}
        options={micOptions}
        onopen={refreshMics}
        onchange={(v) => update((f) => (f.input_device = v === "" ? null : v))}
      />
    {/snippet}
  </SettingsRow>
  <SettingsRow title="Start with Windows" labelFor="autostart" description="Bolo waits in the tray, ready for your push-to-talk key.">
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
        options={Object.entries(LANGUAGE_NAMES)}
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
