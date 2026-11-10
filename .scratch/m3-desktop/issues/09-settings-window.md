# 09: Settings and history window

**What to build:** The settings window from the approved design, backed by `bolo-store`, with changes applied live.

**Blocked by:** 02, 04, 07

**Status:** ready-for-agent

- [ ] General: push-to-talk key (with a "press a key" capture), start with Windows (Tauri autostart plugin, off by default), language mode, Hinglish script.
- [ ] Engines and keys: the engine chain from the manifest, installed models and their sizes, a download for others, API keys (masked, stored with `Secrets`, with a test-key button).
- [ ] Polish: global on/off, tone, and the per-app list with overrides. It explains that polish sends text to Anthropic.
- [ ] Dictionary: add, remove and import a list of words.
- [ ] History: search, copy final or polished text, delete a row, clear all, retention days, off switch.
- [ ] Changes are saved to the TOML and pushed to the engine with `update_config` without a restart.
