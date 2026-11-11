<script lang="ts">
  import { api, keyFromCode, keyLabel, type Form, type SettingsView } from "../lib/api";
  import Segmented from "../lib/Segmented.svelte";

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

<div class="group">
  <div class="card">
    <div class="card-text">
      <span id="ptt">Push-to-talk key</span>
      <span class="caption">
        {#if capturing}
          Press the key you want, or Esc to keep {keyLabel(view.form.push_to_talk)}.
        {:else if view.form.push_to_talk === "caps-lock"}
          Hold to talk, release to insert. A quick tap still toggles Caps Lock. Esc cancels.
        {:else}
          Hold to talk, release to insert. Esc cancels.
        {/if}
      </span>
      {#if captureNote}<span class="caption" style="color: var(--bolo-warn)" role="status">{captureNote}</span>{/if}
    </div>
    {#if capturing}
      <button type="button" class="btn" onclick={endCapture}>Cancel</button>
    {/if}
    <button type="button" class="btn min-w-32" aria-describedby="ptt" class:primary={capturing} onclick={startCapture}>
      {capturing ? "Press a key…" : keyLabel(view.form.push_to_talk)}
    </button>
  </div>
  <div class="card">
    <div class="card-text">
      <label for="autostart">Start with Windows</label>
      <span class="caption">Bolo waits in the tray. Off by default.</span>
    </div>
    <input
      id="autostart"
      class="switch"
      type="checkbox"
      checked={view.form.autostart}
      onchange={(e) => update((f) => (f.autostart = e.currentTarget.checked))}
    />
  </div>
</div>

<h2 class="section">Language</h2>
<div class="group">
  <div class="card">
    <div class="card-text">
      <span>Language mode</span>
      <span class="caption">Auto listens for English and Hinglish together and keeps the better one.</span>
    </div>
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
  </div>
  <div class="card">
    <div class="card-text">
      <span>Hinglish script</span>
      <span class="caption">How Hindi words are written.</span>
    </div>
    <Segmented
      label="Hinglish script"
      value={view.form.script}
      options={[
        ["roman", "Roman"],
        ["devanagari", "देवनागरी"],
      ]}
      onchange={(v) => update((f) => (f.script = v as Form["script"]))}
    />
  </div>
</div>
