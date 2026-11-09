# Architecture

## Crates

| crate | role | status |
|---|---|---|
| `bolo-core` | Session state machine, pipeline orchestration, public `Engine` API | API surface (M1); orchestration lands in M1 phase 2 |
| `bolo-audio` | 16 kHz mono pipeline; WAV encode/decode; FFT resampling (rubato); live capture (cpal, feature `capture`) | encode/decode/resample ✅; capture next |
| `bolo-stt` | `SttEngine` trait; local (sherpa-onnx), Groq, Deepgram, Sarvam; fallback chain with per-engine timeout | ✅ |
| `bolo-models` | `models/manifest.json` registry; resumable SHA-256-verified downloader (GitHub API fallback); RAM-tier recommendation | ✅ |
| `bolo-format` | Rules engine, LLM cleanup, formatter, Devanagari→Roman transliteration | M2 |
| `bolo-store` | Settings TOML, SQLite history, secrets | M3 |
| `bolo-ffi` | UniFFI → Kotlin | M5 (not a default workspace member) |
| `bolo-bench` | Benchmark CLI | ✅ |

Dependency direction: `core → {audio, stt, format}`, `stt → {audio, models}`, `bench → {audio, stt, models}`. Only `apps/*` contain platform UI code.

## Pipeline

```
hotkey down / mic tap ──► prewarm(): local model load  |  HEAD to provider (TLS + h2 ready)
capture (cpal) 16 kHz f32 ──► Silero VAD ──► STT
     streaming (Nemotron): frames → OnlineStream, partials → on_partial
     batch (Parakeet/Qwen3/Moonshine/Whisper/cloud): each VAD segment decoded at the pause
hotkey up ──► finalize last segment ──► rules (<1 ms) ──► [LLM, skippable] ──► format ──► insert
```

## Engine lifecycle (RAM/battery)

- `LocalEngine` holds `Mutex<Option<Recognizer>>`. Weights are loaded by `prewarm()` or on first use and dropped by `release()`.
- The host calls `release()` after its idle timeout (30 s desktop, 20 s Android) and immediately on `onTrimMemory`.
- Inference threads default to `min(4, cores)`.
- Cloud engines share one `reqwest::Client` (keep-alive, h2) so `prewarm()` actually saves the TLS handshake.
- `FallbackEngine` warms only the primary engine. Warming a local fallback would cost RAM for nothing.

## Model registry

`models/manifest.json` is compiled into the binary (`include_str!`) and validated at startup:
- unique ids
- 64-hex SHA-256 values
- https-only URLs
- relative file paths
- defaults that resolve to real entries

Each entry records family (which sherpa config), languages, the script Hindi comes out in, license and whether it may be redistributed, download size and hash, the minimum sherpa-onnx version, minimum RAM, and device tiers.

Install layout: `<models_root>/<id>/…` plus a `.bolo-installed` marker holding the archive's SHA-256. The marker is written last, so a partially installed model never counts as installed.

## Links
- [DECISIONS.md](DECISIONS.md): deviations from the spec and their reasons
- [BENCH.md](BENCH.md): benchmark methodology
