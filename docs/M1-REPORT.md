# M1 report: core engine, benchmarks, first release

**Date:** 2026-11-09. **Release:** `v0.1.0` (Windows x64 CLI preview, prerelease: unsigned, ADR-0013).

M1 delivers the Rust core every later milestone builds on: a live dictation engine with on-device and cloud speech-to-text, English/Hinglish output in Roman script, the benchmark harness that picked the defaults, and the release pipeline. There is no end-user app yet. The `bolo` CLI is the M1 product.

## What shipped

| area | what | where |
|---|---|---|
| Engine | `Engine` API: prewarm, start/stop/cancel dictation, partial and final events, state Idle → Listening → Processing → Done → Idle. Warm mic with pre-roll, Silero VAD segment-ahead decoding, fallback chain with per-engine timeout. | `bolo-core` |
| Language mode | **Auto** (default): Nemotron decodes English and Hindi in parallel on one loaded model, and the language is picked once per dictation from Hindi function-word density. Ambiguous dictations reuse the previous language. English and Hinglish can be pinned. | ADR-0014, `bolo_format::language` |
| Hinglish output | Devanagari → Roman Hinglish transliteration, plus a phonetic lexicon that writes English loanwords in English (ऑफिस → office). | ADR-011, `bolo_format` |
| STT engines | Local (sherpa-onnx): Nemotron streaming, Parakeet v2/v3, Qwen3-ASR, Moonshine-tiny. Cloud, bring your own key: Groq, Deepgram batch, **Deepgram live streaming**, Sarvam. | `bolo-stt` |
| Models | Manifest registry, resumable SHA-256-verified downloads, RAM-tier recommendation. Nothing is bundled. | `bolo-models` |
| CLI | `bolo dictate` (Enter to start/stop, live partials, clipboard, `--lang`, `--polish`), `bolo replay` (plays clips through the live engine), and the benchmark commands (`run`, `transcribe`, `download`, `models`, `sysinfo`, …). | `crates/bolo` |
| Tests | The engine runs in CI on a scripted fake STT and a replayed waveform, with no models and no network: language choice, previous-language fallback, forced modes, cancel and the state sequence. | `crates/bolo-core/tests` |
| Trunk | Protected `main`: squash-merged PRs only, Conventional Commit titles linted, both Rust jobs + `cargo deny` required. Crates are `publish = false`. | ADR-0013 |
| Releases | `scripts/release.py` prepares the release PR (version bump + git-cliff changelog). Pushing the tag builds, smoke-tests and publishes the Windows zip. Proven with `v0.1.0-beta.1`. | `.github/workflows/release.yml` |

## Accuracy and latency

All numbers are from the maintainer's 40 clips (20 English, 20 Hinglish), recorded on the dev laptop's mic, on a Windows desktop in India. Details and methodology: [BENCH.md](BENCH.md).

### Shipping configuration

| engine | mode | WER en | WER hinglish | release → text p50 / p95 |
|---|---|---|---|---|
| **`local:nemotron-stream` (default)** | **Auto** | **8.0%** | **27.7%** | 234 / 898 ms (engine-side p50 200 ms) |
| `local:nemotron-stream` | English | 8.0% | 83.0% | 243 / 4295* ms (engine-side 201 ms) |
| `local:nemotron-stream` | Hinglish | 67.6% | 27.7% | 196 / 338 ms (engine-side 188 ms) |
| `deepgram-stream` (cloud) | Auto (`multi`) | 9.2% | 21.0% | 299 / 370 ms |
| `deepgram` batch (cloud) | Auto (`multi`) | 7.6% | 22.8% | 1306 / 1907 ms |

Measured with `bolo replay`, which plays each clip into the live engine in real time and times stop → final text. \* One machine stall.

Auto picked the right language on **40 of 40** clips, so its accuracy equals the correctly pinned language in both categories. Bars: English ≤ 8.5% ✅, Hinglish ≤ 30% ✅, p50 ≤ 180 ms ❌ (see known gaps).

### Engine comparison (decides defaults)

From `bolo run` (finish-only latency for streaming engines, full decode for batch; cloud is one pre-warmed request). Hinglish output transliterated to Roman.

| engine | WER en | WER hinglish | p50 / p95 ms | peak RSS | download |
|---|---|---|---|---|---|
| `local:nemotron-stream` | 8.0% | 31.2%† | 158 / 171 | 757 MB | 475 MB |
| `local:parakeet-v2-en` | 4.6% | 69.6% (English-only) | 361 / 555 | 795 MB | 482 MB |
| `local:qwen3-asr` | 5.3% | 32.1% | 1429 / 3599 | 1688 MB | 879 MB |
| `local:moonshine-tiny` | 15.6% | 95.1% (English-only) | 118 / 207 | 111 MB | 30 MB |
| `deepgram` (Nova-3, batch) | 4.6% | 20.1% | 660 / 915 | – | – |

† Nemotron's built-in language detection, before Auto mode. Auto brings this to 27.7%.

**Optional LLM cleanup** (`--polish`, Claude Haiku): Nemotron Hinglish WER 31.2% → 14.7%, at +1043 ms p50. It never blocks the instant text (ADR-012).

## Known gaps

1. **Release → text misses 180 ms on the end-to-end clock.** Every mode lands at about 200–240 ms p50 in `bolo replay`. The engine-side work matches the 160–170 ms the bar was set from. The rest is the engine thread finishing an in-progress chunk decode (0–190 ms) before it handles Stop. Fixing that is engine work, not Auto.
2. **Manual checks still owed by a human:**
   - `bolo dictate` spoken in English and Hinglish across several dictations (ticket 05; scripted live-mic runs passed);
   - the release zip running `bolo --help` on a clean Windows profile (ticket 07; CI runs it with only `System32` on `PATH`).
3. **Deepgram `multi` sometimes writes Hindi in Urdu script** ("تیم" for "team"). The romanizer handles only Devanagari.
4. **Not re-benchmarked after transliteration:** Groq, Sarvam and `parakeet-v3`. `qwen3-asr-hinglish` and `whisper-base-hinglish` need our own sherpa-onnx export.
5. **Unsigned, CLI only, Windows only.** No installer, no app, no settings file or history (M3). Linux builds and passes CI but isn't packaged.
6. **Benchmarks are one speaker, one mic.** They pick defaults for this project. They are not general accuracy claims.

## What M2 starts with

M2 is the formatting layer (`bolo-format`): turning raw recognition into text you'd send.

- **Rules engine with golden tests**: fillers, self-corrections ("actually no, make it Friday"), numbers, lists and punctuation, all applied in under 1 ms on the instant path.
- **LLM cleanup as Layer 2**, never blocking insertion (ADR-012). Per-VAD-segment cleanup during long dictations, and the meta-reply/length guards kept under test.
- **Script fixes**: Urdu-script output from Deepgram `multi` (gap 3).
- **Candidate early fix:** let Stop interrupt the engine thread between chunk decodes (gap 1), so the 180 ms bar holds end to end.

Work stops here until M1 is signed off.
