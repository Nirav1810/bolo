# apps/desktop: Bolo for Windows

Tauri v2 + Svelte 5 + Vite + Tailwind. The Rust side (`src-tauri`, crate `bolo-desktop`) hosts one `bolo_core::Engine` in-process. The webviews only render what the Rust app controller sends them.

M3 status: the shell (ticket 01). Bolo sits in the tray, and **Start dictation** in the tray menu starts and stops a dictation. The text goes to the log and to the debug window. Push-to-talk, insertion, the overlay pill and settings come in later M3 tickets (`.scratch/m3-desktop/`).

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
- `src-tauri/src/desktop.rs`: the `Desktop` seam (window in front, clipboard, keys) and its test fake.
- `src-tauri/src/config.rs`: the `BOLO_*` variables.

## Checks

`bolo-desktop` is a workspace member but not a default member, so a plain `cargo build` or `cargo test` skips it. CI's `desktop (windows-latest)` job runs `npm ci`, `npm run check` and `npm run build`, then `cargo clippy -p bolo-desktop --all-targets -- -D warnings` and `cargo test -p bolo-desktop`. The app has no version of its own. Tauri takes the crate's version, which is the workspace's, and `scripts/release.py check` fails if `tauri.conf.json` or `package.json` sets one.
