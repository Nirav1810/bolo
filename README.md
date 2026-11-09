# Bolo

Open-source, local-first voice dictation for **Windows** and **Android**. Hold a hotkey (or tap the mic on the Bolo keyboard), speak English or Hinglish, and release. Polished text appears in whatever app you're typing in.

- **Local-first**: works fully offline with on-device models (sherpa-onnx).
- **Bring your own key**: optional Groq, Deepgram or Sarvam for speech, and Claude Haiku for cleanup.
- **Private**: no telemetry, no accounts. Audio leaves the device only if you configure a cloud provider.
- **Lean**: models load only while you dictate and unload when idle.

> **Status: Milestone 1 (core + benchmark).** There is no end-user app yet. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

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
| Anthropic | (M2) | Claude Haiku cleanup |

In the apps, keys go to Windows Credential Manager or the Android Keystore. They are never written to plain files or logs.

## Models

Models are downloaded on demand from upstream, SHA-256 verified, and never bundled. See [models/manifest.json](models/manifest.json) for each model's license. All current entries allow redistribution, and non-commercial models are excluded.

## Contributing

`main` is protected and every commit on it is releasable ([ADR-0013](docs/adr/0013-single-version-trunk-releases.md)):

1. Work on a branch.
2. Open a PR whose **title is a Conventional Commit**: `type(scope): description`. The types are `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `ci`, `build`, `chore` and `revert`. The scope is optional and `!` marks a breaking change, e.g. `feat(core): add Auto language mode`. CI lints the title.
3. Once CI is green, **squash-merge** it. The PR title becomes the commit on `main`, and the changelog is generated from those commits.

Direct pushes, force pushes and merge commits on `main` are blocked. GitHub's Revert button titles its PR `Revert "…"`, so rename it to `revert: …` before merging.

## License

MIT OR Apache-2.0.
