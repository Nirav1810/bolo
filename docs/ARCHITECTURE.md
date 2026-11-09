# Architecture

## Crates

| crate | role | status |
|---|---|---|
| `bolo-core` | Dictation state machine, pipeline orchestration, public `Engine` API | ✅ live engine, Auto language mode |
| `bolo-audio` | 16 kHz mono pipeline; WAV encode/decode; FFT resampling (rubato); live capture (cpal, feature `capture`) | ✅ |
| `bolo-stt` | `SttEngine` trait; local (sherpa-onnx), Groq, Deepgram (batch and live WebSocket), Sarvam; fallback chain with per-engine timeout | ✅ |
| `bolo-models` | `models/manifest.json` registry; resumable SHA-256-verified downloader (GitHub API fallback); RAM-tier recommendation | ✅ |
| `bolo-format` | Devanagari→Roman transliteration, loanword lexicon, language detector, LLM cleanup guard ✅; rules engine and formatter M2 |
| `bolo-store` | Settings TOML, SQLite history, secrets | M3 |
| `bolo-ffi` | UniFFI → Kotlin | M5 (not a default workspace member) |
| `bolo` | CLI: `dictate` and `replay` on the engine, plus the benchmark subcommands (`run`, `transcribe`, `download`, …) | ✅ |

Dependency direction: `core → {audio, stt, format}`, `stt → {audio, models}`, `bolo (CLI) → {core, audio, stt, models, format}`. Only `apps/*` contain platform UI code.

## Pipeline

```
hotkey down / mic tap ──► prewarm(): local model load  |  HEAD to provider (TLS + h2 ready)
capture (cpal) 16 kHz f32 ──► Silero VAD ──► STT
     streaming (Nemotron): frames → OnlineStream, partials → on_partial
     streaming (deepgram-stream): PCM16 frames → WebSocket, interim results → on_partial
     batch (Parakeet/Qwen3/Moonshine/Whisper/cloud): each VAD segment decoded at the pause
hotkey up ──► finalize last segment ──► rules (<1 ms) ──► [LLM, skippable] ──► format ──► insert
```

### Language mode (ADR-0014)

`EngineConfig::language` is a `LanguageMode`: **Auto**, **English** or **Hinglish**.

- **English / Hinglish**: one stream forced to `en` / `hi`. Hinglish output is romanized (Layer 1).
- **Auto on Nemotron** (`SttEngine::per_stream_language`): two streams on the one loaded recognizer, forced to `en` and `hi`, fed the same audio on two threads. `bolo_format::language` measures the share of Hindi function words in the Hindi pass:
  - ≥ 0.20 → Hinglish;
  - ≤ 0.10 → English;
  - fewer than 3 words, or in between → ambiguous, which reuses the previous dictation's language (English the first time). That memory lives in the engine thread for the life of the process.
- **Partials** come from whichever pass the rule currently favours.
- **At release**, if the Hindi pass's latest text already decides the language, only the chosen pass is finished, so Auto adds nothing after release. Otherwise both are finished in parallel and the rule runs on their final text.
- **Auto on other engines**: their own multilingual mode (Deepgram `language=multi`).
- `FormattedOutput::language` reports the choice (`Auto` when the engine decided).

### Deepgram live streaming

The `deepgram-stream` engine spec (`bolo_stt::cloud::DeepgramStreamEngine`) opens one WebSocket per dictation, on its own thread with a single-threaded tokio runtime:

- `accept` turns audio into 40 ms PCM16 frames and queues them, so it stays a channel send. Audio spoken before the connection is up is queued, not lost.
- Interim `Results` replace the running text and final ones append to it. Each change becomes a partial.
- At release it sends `Finalize` then `CloseStream`. The dictation ends on the `from_finalize` result, or on `Metadata` or the close frame when Deepgram sends no flush. The budget is 2.5 s.
- TLS is rustls with ring and the OS verifier, the same trust as the shared reqwest client. The key goes only in the upgrade request's `Authorization` header.
- `transcribe` (used by `bolo run` and fallback chains) goes to the batch endpoint.
- **Fallback.** In a chain, `FallbackEngine::stream` keeps the dictation's audio next to the primary's stream. If the stream fails at release (refused key, dropped connection, timeout), the rest of the chain transcribes that audio.

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
