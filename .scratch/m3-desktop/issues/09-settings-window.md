# 09: Settings and history window

**What to build:** The settings window from the approved design, backed by `bolo-store`, with changes applied live.

**Blocked by:** 02, 04, 07

**Status:** ready-for-human (built; the manual checks below are owed)

- [x] General: push-to-talk key (with a "press a key" capture), start with Windows (Tauri autostart plugin, off by default), language mode, Hinglish script.
- [x] Engines and keys: the engine chain from the manifest, installed models and their sizes, a download for others, API keys (masked, stored with `Secrets`, with a test-key button).
- [x] Polish: global on/off, tone, and the per-app list with overrides. It explains that polish sends text to Anthropic.
- [x] Dictionary: add, remove and import a list of words.
- [x] History: search, copy final or polished text, delete a row, clear all, retention days, off switch.
- [x] Changes are saved to the TOML and pushed to the engine with `update_config` without a restart.

## Comments

**2026-11-11 (agent).** Built as designed in ticket 04: `src/settings/*.svelte` over `src-tauri/src/commands.rs`.

- `form.rs` is the pure part: the `Form` the window edits, `Form::apply` (tidies and keeps the fields the form doesn't cover) and `changes` (what the app must redo). The app now holds its settings, keys, engine config, polish prefs and push-to-talk key behind mutexes, so every change is saved to the TOML and applied at once:
  - engine changes go to `update_config`, which reloads the model on the next dictation;
  - polish changes only replan;
  - a new key reinstalls the hook and renames the idle hint;
  - autostart uses `tauri-plugin-autostart`;
  - history on/off and retention apply at once (the daily prune reads the current retention).
- The push-to-talk capture lets go of the hook while it listens, so Caps Lock reaches the window.
- Models (`models.rs`): the desktop ASR models, the recommended one first, with downloads through `Downloader` (progress on `bolo://download`, cancel keeps the `.part` for resume). Silero VAD comes along with the first model. Ticket 10 reuses this.
- Keys: masked to their last four characters (`Secrets::hint`), with a Test button (`keys.rs`: one authenticated GET to Anthropic, Deepgram or Groq; Sarvam can't be checked ahead of time).
- History: search, Copy (and Copy original when polish changed it), delete a row (`History::delete`), clear all with a confirm, the on/off switch and retention.
- A second launch of the app now opens Settings, not the debug page. The hash picks a tab (`#history`, `#engines`, ...).

**Manual checks owed (human):** change the key to F13 or Right Ctrl and dictate with it; turn Start with Windows on, then check that Bolo appears under Task Manager > Startup apps; save, test and remove a real key; delete a downloaded model and download it again; turn history off and on.
