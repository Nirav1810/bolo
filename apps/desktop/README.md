# apps/desktop: Bolo for Windows

Tauri v2 + Svelte 5 + Vite + Tailwind. The Rust side (`src-tauri`, crate `bolo-desktop`) hosts one `bolo_core::Engine` in-process. The webviews only render what the Rust app controller sends them.

M3 status: the shell (ticket 01), push-to-talk (ticket 05), and insertion with the polish swap (ticket 06). Bolo sits in the tray. **Hold Caps Lock** to dictate and let go to finish (ADR-0017); a quick tap still toggles Caps Lock, Shift+Caps Lock is plain Caps Lock, and Esc cancels a dictation. **Start dictation** in the tray menu works too, and **Pause Bolo** lets go of the key. The overlay pill and settings come in later M3 tickets (`.scratch/m3-desktop/`).

## Insertion (ADR-0018)

The text is pasted where the cursor is: Bolo writes it to the clipboard and sends Ctrl+V. Afterwards it restores your clipboard, 300 ms after the paste, unless you copied something in the meantime.

- **What's kept.** Bolo saves and restores text, HTML, images (`CF_DIBV5`) and copied files (`CF_HDROP`). **Any other clipboard format is lost after a dictation**, for example an Excel range's own formats or rich text without HTML.
- **Clipboard history.** Bolo's writes are marked so they stay out of Win+V history, the cloud clipboard and clipboard managers.
- **Admin windows.** If the window in front runs as administrator and Bolo doesn't, Windows blocks the paste. The text is left on the clipboard instead, and a `notice` event says so.
- **Polish swap.** With polish on, the polished text replaces the instant text only if all of these hold:
  - you haven't pressed a key or clicked since the paste;
  - the same window is in front;
  - polish arrived within 5 s;
  - the text is a single line.

  Bolo selects the instant text back with Shift+Left, then pastes over it. The log records each swap, or why it was skipped, as well as the time from key-up to the paste being sent next to the engine's `total_ms`.

The push-to-talk key is `push_to_talk` in `%APPDATA%\Bolo\settings.toml`: `caps-lock` (the default), `right-ctrl`, `right-alt` or `f13` … `f24`. It is read at startup.

## Run it

```sh
cd apps/desktop
npm ci
npx tauri dev          # Vite dev server + a debug build of the app
```

The sherpa-onnx DLLs must sit next to the exe. Cargo copies them into `target/<profile>` when `sherpa-onnx-sys` builds. Models come from `%LOCALAPPDATA%\Bolo\data\models` (ADR-007); install one with `bolo download nemotron-stream`. The log is `%LOCALAPPDATA%\Bolo\data\logs\desktop.log`.

Until settings land (ticket 02), the engine is configured from environment variables:

| Variable | Value |
|---|---|
| `BOLO_ENGINES` | comma-separated engine specs, e.g. `local:nemotron-stream,deepgram` |
| `BOLO_LANGUAGE` | `auto`, `english` or `hinglish` |
| `BOLO_SCRIPT` | `roman` or `devanagari` |
| `BOLO_POLISH` | `off`, `on`, or a tone (`casual`, `formal`); needs `ANTHROPIC_API_KEY` |
| `BOLO_DICTIONARY` | comma-separated terms |
| `BOLO_MODELS_DIR` | overrides the models directory |
| `<PROVIDER>_API_KEY` | `DEEPGRAM_API_KEY`, `GROQ_API_KEY`, `SARVAM_API_KEY`, `ANTHROPIC_API_KEY` |

## Layout

- `src-tauri/src/controller.rs`: engine events → `AppEvent`s on the `bolo://event` Tauri event (mirrored in `src/lib/events.ts`).
- `src-tauri/src/session.rs`: what start, stop and pause do, tested with a fake engine.
- `src-tauri/src/desktop.rs`: the `Desktop` seam (window in front, clipboard, keys, the UIPI check) and its test fake.
- `src-tauri/src/insert.rs`: paste, clipboard restore and the polish swap, tested against the fake desktop and a fake timer.
- `src-tauri/src/hotkey.rs`: the push-to-talk state machine (pure, fake-clock tests).
- `src-tauri/src/keyhook.rs`: the `WH_KEYBOARD_LL` hook thread that runs it, and the tap replay.
- `src-tauri/src/config.rs`: the `BOLO_*` variables.

## Checks

`bolo-desktop` is a workspace member but not a default member, so a plain `cargo build` or `cargo test` skips it. CI's `desktop (windows-latest)` job runs `npm ci`, `npm run check` and `npm run build`, then `cargo clippy -p bolo-desktop --all-targets -- -D warnings` and `cargo test -p bolo-desktop`.

Two tests touch the real desktop and are `#[ignore]`d: a clipboard round trip, and a paste-and-swap into an EDIT control the test opens. Run them with `cargo test -p bolo-desktop -- --ignored --test-threads=1` (they share the clipboard), from a visible console on the interactive desktop. Keys sent from a background or remote shell never arrive. The app has no version of its own. Tauri takes the crate's version, which is the workspace's, and `scripts/release.py check` fails if `tauri.conf.json` or `package.json` sets one.
