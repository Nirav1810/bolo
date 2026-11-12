# Architecture

## Crates

| crate | role | status |
|---|---|---|
| `bolo-core` | Dictation state machine, pipeline orchestration, public `Engine` API, polish (`on_polished`, per segment) | ✅ live engine, Auto language mode, polish |
| `bolo-audio` | 16 kHz mono pipeline; WAV encode/decode; FFT resampling (rubato); live capture (cpal, feature `capture`) | ✅ |
| `bolo-stt` | `SttEngine` trait; local (sherpa-onnx), Groq, Deepgram (batch and live WebSocket), Sarvam; fallback chain with per-engine timeout | ✅ |
| `bolo-models` | `models/manifest.json` registry; resumable SHA-256-verified downloader (GitHub API fallback); RAM-tier recommendation | ✅ |
| `bolo-format` | Devanagari and Urdu → Roman transliteration, loanword lexicon, language detector, Layer 1 rules pipeline (`rules`), LLM cleanup request and guard | ✅ |
| `bolo-store` | Versioned settings TOML (migrations, bad-file backup), SQLite history (30-day prune), API keys in the OS credential store | ✅ |
| `bolo-ffi` | UniFFI → Kotlin | M5 (not a default workspace member) |
| `bolo` | CLI: `dictate` and `replay` on the engine, plus the benchmark subcommands (`run`, `transcribe`, `download`, …) | ✅ |

Dependency direction: `core → {audio, stt, format}`, `stt → {audio, models}`, `bolo (CLI) → {core, audio, stt, models, format}`, `store → core`. Only `apps/*` contain platform UI code.

## Windows app (`apps/desktop`, M3–M4)

Tauri v2 hosts one `Engine` in-process; the Svelte webviews only render.

```
WH_KEYBOARD_LL hook ─► hotkey state machine ─► Start / Stop / Cancel
   (Caps Lock, ADR-0017)                          │
foreground exe + title ─► app rules + app profiles (ADR-0020) ─► TargetContext + polish plan ─► Engine
Engine events ─► controller ─► inserter: clipboard paste, restore, polish swap (ADR-0018)
                             ├► journal: SQLite history
                             └► overlay: pill state machine ─► pill webview (waveform, partials)
```

- The decisions are pure and tested without a desktop: the hotkey machine, the swap decision, the app rules, the pill, and the settings form. The Win32 calls sit behind the `Desktop` seam.
- Settings are saved and applied live. Engine changes go through `update_config`. Per-dictation polish goes through `set_polish`, and snippets and the microphone through `set_snippets` and `set_input_device`, so neither reloads the model.
- Snippets are expanded last, after the rules and after polish (`bolo_format::snippets`).
- The settings window is built on demand and destroyed on close (M4 ticket 04). Only the pill's webview lives for the whole session.
- The per-user NSIS installer bundles the sherpa-onnx DLLs. Models are downloaded on first run (onboarding), not shipped. Releases and nightlies update themselves (ADR-0019), and they are Authenticode-signed through SignPath once that's approved (ADR-0021).

## Pipeline

```
hotkey down / mic tap ──► prewarm(): local model load  |  HEAD to provider (TLS + h2 ready)
capture (cpal) 16 kHz f32 ──► Silero VAD ──► STT
     streaming (Nemotron): frames → OnlineStream, partials → on_partial
     streaming (deepgram-stream): PCM16 frames → WebSocket, interim results → on_partial
     batch (Parakeet/Qwen3/Moonshine/Whisper/cloud): each VAD segment decoded at the pause
hotkey up ──► finalize last segment ──► romanize ──► rules (<1 ms) ──► on_final ──► insert
                                                       └─► [polish, Polish::On] ──► on_polished ──► replace
```

### Layer 1 rules (ADR-0015)

`bolo_format::rules::format(text, language, code_mode)` runs once per final, after romanization and the loanword lexicon, and returns the text rendered as `plain`, `markdown` and `html`. Its time is `Timings::rules_ms`. The passes run in a fixed order on one token list:

```
fillers → self-corrections → commands and lists → numbers → punctuation fix-ups → render
```

- **Fillers**: hesitations, stutters, "you know"/"like" between pauses. Hindi fillers only at the start of a Hinglish dictation.
- **Self-corrections**: after "actually no", "I mean", "scratch that", "nahi nahi" and similar markers, the repair replaces the nearest date, time, number or noun phrase, or the restated clause.
- **Commands and lists**: "new line", "new paragraph", "nayi line". An explicit enumeration of three or more items becomes a numbered list.
- **Numbers**: the ADR-0015 number style (digits except one to nine in prose; money, time, dates and % always digits; Indian grouping with lakh, crore or rupees).
- **Punctuation fix-ups and render**: sentence capitals, final period, capital "I", spacing. `code_mode` drops capitals and the final period. Markdown leaves blank lines around a list. HTML writes `<p>` paragraphs, `<br>` line breaks and `<ol><li>` lists, with every text node escaped.

Every pass leaves the text verbatim when unsure. The pipeline is idempotent and never empty for non-empty input (property tests), and `tests/latency.rs` holds it under 1 ms p95. `bolo-format` doesn't depend on `bolo-core`: the core maps `TargetContext` onto `rules::Language` and the `code_mode` flag.

### Polish (ADR-0016)

`EngineConfig::polish` is `Polish::Off` (default) or `Polish::On { tone, app, dictionary }`, which needs the `anthropic` key in `ApiKeys`.

- `on_final` always carries the Layer 1 text first. Polish then runs on the engine's tokio runtime and sends `on_polished` with the polished text in `plain` and `markdown`, and `timings.llm_ms` set. For an HTML target, `html` is the polished text read back through the same renderer (`rules::to_html`); model output is never trusted as markup.
- The deadline is 1.5 s (`POLISH_DEADLINE`). A timeout, an error, a meta-reply or a failed length-ratio guard sends nothing more, and the Layer 1 text stands. The next `start_dictation` or a `cancel_dictation` supersedes a running polish.
- **Per segment** (`segment.rs`): once a dictation reaches 20 s, each pause of 700 ms or more closes a segment, found by Silero VAD or by an energy gate when the model isn't installed. A `Polisher` task polishes the segments in order, each with the previous polished segment as `<context>`. At release only the rest is polished, then the parts are joined. A failed segment keeps its Layer 1 text. If the final text no longer lines up with the segments, the whole text is polished instead.
- The LLM sits behind the `LlmClient` trait (`Engine::with_llm_client`), so engine tests use a fake with no network.

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
- The engine calls `release()` after its idle timeout (20 s on Android) and the host immediately on `onTrimMemory`. The desktop app sets no timeout: it loads the model at launch and keeps it, so a push-to-talk hold never waits on a ~2.4 s load. It releases it only when paused.
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
