# 01: Desktop app shell hosting the engine

**What to build:** The Tauri v2 + Svelte 5 + Vite + Tailwind app in `apps/desktop`. It hosts one `Engine` in-process, has a tray icon, and lets you start and stop a dictation from the tray (a test path until the hotkey lands). CI builds it on Windows.

**Blocked by:** none

**Status:** done

- [x] `apps/desktop/src-tauri` is a workspace member and not a default member. Its version comes from the workspace (ADR-0013), and `scripts/release.py check` knows about it.
- [x] An app controller in Rust implements `EngineEvents` and forwards state, level, partial, final and polished as Tauri events. Dictation logic stays in Rust, and the webviews only render.
- [x] A single instance, and a tray menu: Start/stop dictation (temporary), Pause Bolo, Settings, History, Quit. For now the final text goes to the log and to a debug window.
- [x] The engine config comes from defaults plus `BOLO_*` env vars until ticket 02 lands. Models come from the ADR-007 location, and sherpa DLLs sit next to the exe.
- [x] A `Desktop` trait seam (clipboard, send keys, foreground window), so later tickets test the controller with a fake.
- [x] A new CI job `desktop (windows-latest)`: frontend install and build (pnpm or npm, locked), `cargo clippy -p bolo-desktop -- -D warnings`, `cargo test -p bolo-desktop`. Add it to ruleset 26847972's required checks.
- [x] `cargo deny` still passes with the Tauri dependency tree. Licence exceptions are justified in `deny.toml`.

## Outcome

- `apps/desktop`: Tauri 2.11 + Svelte 5.57 + Vite 8 + Tailwind 4, npm with a lockfile. The crate `bolo-desktop` (`apps/desktop/src-tauri`) is a workspace member, not a default member. `tauri.conf.json` and `package.json` carry no version, so Tauri takes the crate's (the workspace's). `scripts/release.py check` now fails if either one sets a version, and it covers `bolo-desktop` in `Cargo.lock` like every member.
- `controller.rs`: `Controller` implements `EngineEvents` and sends `AppEvent`s (`state`, `paused`, `level`, `partial`, `final`, `polished`, `error`, tagged by `kind`) on the one Tauri event `bolo://event`. `src/lib/events.ts` mirrors the payloads. The final and polished text go to the log at `info` level (temporary, until insertion in ticket 06).
- `session.rs`: `Session::toggle` stops a dictation that is listening, or starts one with the target context of the window in front (Plain until ticket 07). `set_paused` cancels and unloads the model. It is tested against `EngineControl`, a fake-able view of `Engine`.
- `desktop.rs`: the `Desktop` trait (`foreground`, `read_clipboard`, `write_clipboard`, `send_keys` with `Key::{Paste, ShiftInsert, ShiftLeft, CapsLock}`) and `fake::FakeDesktop`, which records writes and keys. `WinDesktop::foreground` is real (exe name, lower-case, plus the title). The clipboard and keys return `DesktopError::Unsupported` until ticket 06.
- `config.rs`: `BOLO_ENGINES`, `BOLO_LANGUAGE`, `BOLO_SCRIPT`, `BOLO_POLISH`, `BOLO_DICTIONARY`, `BOLO_MODELS_DIR` and `<PROVIDER>_API_KEY`. A bad value is logged and shown in the debug window, and the defaults are used.
- ADR-007 location: `bolo_models::default_models_root()`, now shared with the CLI. Log: `%LOCALAPPDATA%\Bolo\data\logs\desktop.log`.
- Tray: Start/Stop dictation, Pause/Resume Bolo, Settings…, History…, Debug window, Quit. Settings and History open placeholder pages in the debug window. Closing a window hides it, and Bolo keeps running until Quit, which drops the engine first. Single instance: a second launch exits and shows the debug window (checked).
- CI: the `rust` jobs exclude `bolo-desktop`. The new job `desktop (windows-latest)` runs `npm ci`, `npm run check` (svelte-check), `npm run build`, then clippy and the tests for `bolo-desktop`. It's added to ruleset 26847972's required checks.
- `cargo deny`: the licences pass unchanged. Nine advisories, all unmaintained crates and none a vulnerability, are ignored with reasons in `deny.toml`:
  - Linux GTK and D-Bus crates, which the Windows-only app never ships;
  - `unic-*` via `tauri-utils` → `urlpattern`, which no Tauri 2 release avoids.
- `npm audit`: 2 low-severity findings, both `deepmerge` under `@sveltejs/vite-plugin-svelte`. That is build-time only and has no fix.
- Owed by the user: a live tray dictation (Start, speak, Stop) with the debug window open. To launch, run `npx tauri dev` in `apps/desktop`.
