# 01: Desktop app shell hosting the engine

**What to build:** The Tauri v2 + Svelte 5 + Vite + Tailwind app in `apps/desktop`. It hosts one `Engine` in-process, has a tray icon, and lets you start and stop a dictation from the tray (a test path until the hotkey lands). CI builds it on Windows.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `apps/desktop/src-tauri` is a workspace member and not a default member. Its version comes from the workspace (ADR-0013), and `scripts/release.py check` knows about it.
- [ ] An app controller in Rust implements `EngineEvents` and forwards state, level, partial, final and polished as Tauri events. Dictation logic stays in Rust, and the webviews only render.
- [ ] A single instance, and a tray menu: Start/stop dictation (temporary), Pause Bolo, Settings, History, Quit. For now the final text goes to the log and to a debug window.
- [ ] The engine config comes from defaults plus `BOLO_*` env vars until ticket 02 lands. Models come from the ADR-007 location, and sherpa DLLs sit next to the exe.
- [ ] A `Desktop` trait seam (clipboard, send keys, foreground window), so later tickets test the controller with a fake.
- [ ] A new CI job `desktop (windows-latest)`: frontend install and build (pnpm or npm, locked), `cargo clippy -p bolo-desktop -- -D warnings`, `cargo test -p bolo-desktop`. Add it to ruleset 26847972's required checks.
- [ ] `cargo deny` still passes with the Tauri dependency tree. Licence exceptions are justified in `deny.toml`.
