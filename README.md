# Bolo

Open-source, local-first voice dictation for **Windows** and **Android**. Hold a hotkey (or tap the mic on the Bolo keyboard), speak English or Hinglish, and release. Polished text appears in whatever app you're typing in.

- **Local-first**: works fully offline with on-device models (sherpa-onnx).
- **Bring your own key**: optional Groq, Deepgram or Sarvam for speech, and Claude Haiku for cleanup.
- **Private**: no telemetry, no accounts. Audio leaves the device only if you configure a cloud provider.
- **Lean**: models load only while you dictate and unload when idle.

> **Status: Milestone 3: the Windows desktop app (preview).** There is no Android keyboard yet. See the [M3 report](docs/M3-REPORT.md), the [M2 report](docs/M2-REPORT.md), the [M1 report](docs/M1-REPORT.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Install the desktop app (Windows x64)

1. Download `Bolo_X.Y.Z_x64-setup.exe` from the [latest release](https://github.com/Nirav1810/bolo/releases). You can check it against the `.sha256` file next to it.
2. Run it. It installs for your user only, so it needs no admin rights. If WebView2 is missing, the installer downloads it (Windows 11 already has it).
3. The installer isn't code-signed yet, so **Windows SmartScreen may say "Windows protected your PC"**. Click **More info**, then **Run anyway**. This happens only for unsigned downloads; the `.sha256` file lets you confirm it's the published file.
4. On first run, Bolo walks you through a mic check and downloads the speech model (about 475 MB, once). It then waits in the tray.

**Using it.** Click where you want the text, **hold Caps Lock**, speak, and let go. The text is pasted at the cursor, and a small pill at the bottom of the screen shows what Bolo hears.
- A quick tap of Caps Lock (under 150 ms) still toggles capitals, and Shift+Caps Lock is always plain Caps Lock.
- **Esc** while talking cancels.
- You can pick another key (Right Ctrl, Right Alt, F13–F24) in Settings.
- Lists arrive as real lists in Word, Outlook and Gmail, and as Markdown in Slack, Notion and Obsidian. Code editors and terminals get no auto-capitals or trailing period.
- Your clipboard is restored after each paste, and the pastes stay out of Win+V history.

**Privacy.** With the default local model, audio and text never leave the PC. Audio goes to a provider only if you choose a cloud engine (Deepgram, Groq, Sarvam). Polish, which is off by default, sends the text, never the audio, to Anthropic using your own key; it stays off in code editors and terminals. API keys are kept in Windows Credential Manager. History keeps text only, on this PC, for 30 days, and you can turn it off or clear it. Bolo has no telemetry and no accounts.

To uninstall, use Windows Settings › Apps. The uninstaller asks whether to delete your downloaded models and dictation history. Settings and API keys stay unless you tick "Delete the application data".

## The CLI (Windows x64)

1. Download `bolo-vX.Y.Z-windows-x64.zip` from the [latest release](https://github.com/Nirav1810/bolo/releases) (optionally check it against the `.sha256` next to it) and unzip it anywhere. Keep `bolo.exe` next to the DLLs.
2. In PowerShell, in that folder:

   ```powershell
   .olo.exe sysinfo                      # RAM and the recommended model
   .olo.exe download nemotron-stream     # ~475 MB, SHA-256 verified
   .olo.exe dictate                      # Enter starts and stops; the text goes to the clipboard
   ```

`dictate` uses Auto language mode: speak English or Hinglish and Bolo picks one per dictation. Pin it with `--lang en` or `--lang hi`. The text is formatted on the way out: fillers removed, spoken corrections applied ("Tuesday, actually no, Wednesday" → "Wednesday"), numbers as digits ("₹1,25,000", "5:30 pm"), spoken lists as numbered lists, and "new line" or "new paragraph" as breaks. With `--polish` and `ANTHROPIC_API_KEY` in `.env`, Claude polishes the text after the instant result. If the polished text arrives within 1.5 s, it replaces the instant text on the clipboard. For a cloud engine, put `DEEPGRAM_API_KEY=…` in a `.env` file in that folder and run `.olo.exe dictate -e deepgram-stream`. Releases are unsigned previews, so SmartScreen may warn on first run.

**M2 numbers** (40 clips, one speaker; formatted WER compares with the text you'd send; details in the [M2 report](docs/M2-REPORT.md)):

| engine | fmt WER English | fmt WER Hinglish | release → text p50 |
|---|---|---|---|
| `local:nemotron-stream`, Auto (default, offline) | 9.0% | 30.9% | 129–182 ms |
| same + polish (Claude Haiku, BYOK) | 8.6% | 15.7% | +1 s; 128 ms per segment on 60 s dictations |
| `deepgram` (Nova-3 batch, cloud, BYOK) | 4.3% | 21.2% | 1306 ms (`deepgram-stream`: 290 ms) |

## Build (developers)

Requirements: Rust stable (≥1.88), Python 3 (for the lib fetch script), and on Windows the MSVC build tools (VS 2022).

```sh
python -I scripts/fetch-sherpa-libs.py      # verified prebuilt sherpa-onnx libs → .cache/
cargo test --workspace
cargo build --release -p bolo
```

## Benchmark engines on your own voice

See [docs/BENCH.md](docs/BENCH.md). In short:

```sh
bolo download nemotron-stream moonshine-tiny
bolo transcribe my.wav -e local:nemotron-stream
bolo run --engines local:nemotron-stream,groq,sarvam
```

## BYOK setup

For the developer tools, copy `.env.example` to `.env` (git-ignored) and paste your keys there. The `bolo` CLI loads it automatically.

| provider | env var | used for |
|---|---|---|
| Groq | `GROQ_API_KEY` | Whisper large-v3-turbo STT |
| Deepgram | `DEEPGRAM_API_KEY` | Nova-3 STT: `deepgram` (batch) or `deepgram-stream` (live WebSocket) |
| Sarvam AI | `SARVAM_API_KEY` | Saaras v4 STT, Roman Hinglish (`translit`) |
| Anthropic | `ANTHROPIC_API_KEY` | Claude Haiku polish (`bolo dictate --polish`) |

In the apps, keys go to Windows Credential Manager or the Android Keystore. They are never written to plain files or logs.

## Models

Models are downloaded on demand from upstream, SHA-256 verified, and never bundled. See [models/manifest.json](models/manifest.json) for each model's license. All current entries allow redistribution, and non-commercial models are excluded.

## Contributing

`main` is protected and every commit on it is releasable ([ADR-0013](docs/adr/0013-single-version-trunk-releases.md)):

1. Work on a branch.
2. Open a PR whose **title is a Conventional Commit**: `type(scope): description`. The types are `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `ci`, `build`, `chore` and `revert`. The scope is optional and `!` marks a breaking change, e.g. `feat(core): add Auto language mode`. CI lints the title.
3. Once CI is green, **squash-merge** it. The PR title becomes the commit on `main`, and the changelog is generated from those commits.

Direct pushes, force pushes and merge commits on `main` are blocked. GitHub's Revert button titles its PR `Revert "…"`, so rename it to `revert: …` before merging.

### Releasing

One version for everything, cut from `main` as `vX.Y.Z` (stable) or `vX.Y.Z-beta.N` (beta prerelease). Before 1.0 every release is marked as a prerelease, because the Windows build isn't code-signed yet. You need [git-cliff](https://git-cliff.org) (`pip install git-cliff`) and the GitHub CLI.

```sh
python -I scripts/release.py 0.1.0 --dry-run   # show the version bump and changelog diff
python -I scripts/release.py 0.1.0             # branch, bump, regenerate CHANGELOG.md, open the PR
# after the `chore(release): v0.1.0` PR is squash-merged, on the updated main:
python -I scripts/release.py tag 0.1.0         # tag and push; .github/workflows/release.yml publishes
```

The tag workflow builds `bolo` for Windows x64 and zips it with the sherpa-onnx DLLs, `README.txt` and the licences. It runs the unzipped `bolo --help` with nothing but Windows on `PATH`. It also builds the desktop app's per-user NSIS installer (`npx tauri build` in `apps/desktop`), installs and uninstalls it silently, and then publishes the GitHub Release with that version's `CHANGELOG.md` section as notes. Don't edit `CHANGELOG.md` by hand. Each release PR regenerates it from the commit titles ([cliff.toml](cliff.toml)), and a stable release folds its betas into one section.

## License

MIT OR Apache-2.0.
