# Architecture decisions

ADR-style log: newest at the bottom. Each entry records deviations from the original build spec and the reasons for them.

---

## ADR-001 — Official `sherpa-onnx` Rust crate (k2-fsa), pinned `=2.0.1`
**Status:** accepted (M1)

**Context:** we need a local runtime for streaming transducers, NeMo TDT, Qwen3-ASR, Moonshine v2, Whisper, and Silero VAD.

**Decision:** use `k2-fsa`'s own `sherpa-onnx` crate (Apache-2.0) rather than the community `sherpa-rs`. It has `OfflineQwen3ASRModelConfig`, Moonshine v2 (`merged_decoder`), and online/offline transducers. Pin it exactly, because model exports are tied to runtime versions:
- Nemotron 3.5 needs ≥1.13.5.
- Qwen3-ASR needs ≥1.13.7 (fixes hallucination on silence).
- Moonshine v2 needs ≥2.0.0 (attention-mask fix).

## ADR-002 — Model registry corrections vs. spec
**Status:** accepted (M1). Verified 2026-11-09 against GitHub release digests and the HF API.

| spec said | reality | action |
|---|---|---|
| "Nemotron 3.5 ASR Streaming 0.6B int8", English + Hindi | Real (`nvidia/nemotron-3.5-asr-streaming-0.6b`, OpenMDW-1.1). hi-IN is one of 19 transcription-ready locales, and the language is set per stream. Code-switched Hinglish is **not** claimed, and Hindi is written in Devanagari. Exported in 5 chunk sizes. | `nemotron-stream` = 320 ms (default) and `nemotron-stream-560` (bench candidate) |
| Parakeet TDT 0.6B v3 is English-only | v3 covers **25 European languages** and has no Hindi. v2 is the English-only model, with lower English WER. | Kept `parakeet-v3` and added `parakeet-v2-en` |
| Qwen3-ASR Srota Hinglish fine-tune "(ONNX)" | Real (`moorlee/qwen3-asr-0.6b-hinglish`, Apache-2.0), but **safetensors only**: no ONNX anywhere. Output mixes Devanagari and Latin. | `status: needs-export`. Added the base `qwen3-asr` (sherpa export exists) as a stand-in. |
| Moonshine v2 Tiny int8 | Exists as "quantized" `.ort` (MIT, English). Non-English Moonshine models are **non-commercial**. | `moonshine-tiny`; never add non-English Moonshine |
| Whisper-base Hinglish fine-tune | `Oriserve/Whisper-Hindi2Hinglish-Swift` (Apache-2.0) emits **Roman** Hinglish, which is ideal, but no sherpa-format export exists (the HF ONNX copies use the optimum layout). | `status: needs-export` via HF → OpenAI `.pt` → sherpa `export-onnx.py` |
| (VAD) | Silero VAD (MIT) is in the `asr-models` release. TEN-VAD is Apache-2.0 **plus non-compete terms**. | Silero only; TEN-VAD rejected |

Licenses of all shipped-by-download models allow redistribution. We still download from upstream URLs and mirror only our own exports.

## ADR-003 — sherpa-onnx link mode: shared by default
**Status:** accepted (M1)

**Context:** the prebuilt Windows **static** lib (`static-MT`) references `__std_find_first_of_trivial_pos_1` from a newer MSVC STL. It fails to link with VS 2022 17.13 / MSVC 14.43, which is on the dev machine, and it also requires the static CRT.

**Decision:** `bolo-stt` feature `sherpa-shared` is the default. `sherpa-onnx-sys` copies `sherpa-onnx-c-api.dll`, `onnxruntime.dll` and related DLLs next to the binary. `sherpa-static` remains opt-in for single-exe builds on MSVC ≥ 14.44 with `+crt-static`. Android is always shared (`.so` in `jniLibs`).

**Follow-up (M3):** measure installer size for both modes against the 15 MB budget. onnxruntime dominates. `no-tts` lib variants exist upstream and are ~3–5% smaller.

## ADR-004 — TLS: rustls + `ring`, not reqwest's default aws-lc-rs
**Status:** accepted (M1)

**Decision:** use reqwest 0.13 with `rustls-no-provider` and install the `ring` provider once. aws-lc-sys needs cmake/NASM on some targets, which complicates MSVC and the Android NDK. `ring` builds with plain `cc` and is smaller. HTTP/2 is negotiated via ALPN; all three providers' hosts advertise `h2`.

## ADR-005 — Cloud API shapes (verified 2026-11-09)
| provider | endpoint | notes |
|---|---|---|
| Groq | `POST /openai/v1/audio/transcriptions`, multipart | `whisper-large-v3-turbo`, $0.04/hr, **10 s minimum billed per request**, prompt ≤224 tokens. We do not force `language=hi` for Roman Hinglish (it yields Devanagari); we bias the script through the prompt instead. |
| Deepgram | `POST /v1/listen`, raw body, `Authorization: Token` | `nova-3`. `language=multi` includes Hindi code-switching; `keyterm` is repeated per term. $0.0052/min multilingual, +$0.0013/min keyterm. The Hindi output script is undocumented, so the bench measures it. |
| Sarvam | `POST /speech-to-text`, multipart, `api-subscription-key` | **`saaras:v4`** (spec named Saaras without a version). **`mode=translit`** gives Roman Hinglish (`codemix` keeps Devanagari). REST max 30 s, so we chunk at 29 s as a backstop. `keyterms` is v4-only. Docs contradict each other on whether `mode` works on v4; bench verifies. |
| Anthropic | — | `claude-haiku-5-5` confirmed current (used in M2). |

## ADR-006 — Cloud upload format: WAV for now
**Status:** provisional

The spec asked for Opus/OGG uploads. libopus needs cmake or a C build per target, and Groq's docs say WAV has the lowest server-side latency. 16 kHz 16-bit mono is 32 KB/s, which is fine on Wi-Fi/LTE for ≤30 s clips. **Revisit in M5** with the bench on a mobile connection. Candidates are pure-Rust FLAC (`flacenc`, ~50% smaller, no C) or Opus if upload time dominates.

## ADR-007 — Model storage location
Models go to the **local** (non-roaming) app-data dir: `%LOCALAPPDATA%\Bolo\data\models` on Windows and `filesDir/models` on Android. Settings stay in `%APPDATA%\Bolo\`, as in the spec. Hundreds of MB must not sync with roaming profiles.

## ADR-008 — Download resilience and supply-chain checks
- GitHub's `github.com/<o>/<r>/releases/download/...` route returned 504 for hours on 2026-11-09 while the API asset route worked. The model downloader falls back to the API route automatically. The SHA-256 check is unchanged, so this is safe.
- `sherpa-onnx-sys` downloads its prebuilt libs **without verification**. We pre-fetch the exact archive with `scripts/fetch-sherpa-libs.py`, which checks the release `digest`. `.cargo/config.toml` points `SHERPA_ONNX_ARCHIVE_DIR` at the result. CI does the same.

## ADR-009 — Microphone start-up dead time (open question for M3)
**Status:** measured; the policy decision is pending.

`bolo-bench mic-test` on the dev laptop (Realtek "Microphone Array", 48 kHz) gives the same result on cold open, a re-open after 0.2 s, and a re-open after 3 s: `open()` takes 75–93 ms, then the device delivers **exactly 500 ms of digital zeros**. The cause is most likely the driver's beamforming/noise-suppression processing.

Anything said in the first ~0.6 s after the stream opens is lost. In the first bench recordings this clipped the opening word of many clips ("Hey Priya…" became "Quick reminder…").

Options for the desktop app:
1. Keep the input stream open while Bolo runs, discarding samples into a ring buffer. This gives pre-roll and no lost words, but the Windows mic indicator stays on and idle CPU is small but non-zero.
2. Open on hotkey-down and show a "ready" cue only once real samples arrive. This uses zero idle resources, but the user waits ~0.6 s.
3. A hybrid: keep the stream warm for N minutes after each use, and fall back to option 2 when cold.

Also check whether turning off Windows "Audio enhancements" for the device removes the delay.

The recorder already applies option 2: it waits for non-zero samples before saying "speak now" and trims the dead lead-in.

## ADR-010 — Scoring fairness: numbers and script
- WER/CER canonicalize numbers on both sides before comparing. "4 pm" = "four p.m.", "1,250" = "one thousand two hundred fifty", and "6:30" = "six thirty". Before this, correct transcripts were penalized up to 56% WER.
- Devanagari hypotheses scored against Roman references go through `bolo_format::translit` first. This is the same pass the app will use for Roman-Hinglish output. `clips.csv` keeps both the raw and the scored text.
- Tried and rejected: 0.3 s of leading silence for Nemotron streams. English WER went from 8.0% to 12.6% because it shifts chunk alignment.

### ADR-009 update (2026-11-09)
Turning off Windows **Audio enhancements** for the Realtek mic removed the dead time entirely. Leading digital silence went from 500 ms to **0 ms**. `open()` now takes 313 ms cold and about 38 ms warm.

Decision for M3: use the **hybrid** policy. Keep the input stream warm for about 2 minutes after each dictation, then close it, and show a "ready" cue only once real samples arrive. Onboarding should suggest disabling Audio enhancements when `mic-test`-style probing detects dead time.

## ADR-011 — English loanwords in Devanagari output: generated phonetic lexicon
Local engines (Nemotron, Qwen3) write English words in Devanagari (ऑफिस, डॉक्टर). Plain transliteration gives "ophis" and "doktar".

`bolo_format::loanwords` restores the English spelling:
- **Lexicon:** about 41k words from SCOWL tiers 10–50 (English + British spellings; capitalized words limited to days and months), with pronunciations from CMUdict. Both sources are permissive; notices are in `crates/bolo-format/data/README.md`.
- **Matching:** both sides map to a coarse phonetic key. A match needs an identical consonant skeleton, limited vowel slack (0 edits for ≤2 consonants, 1 for 3–4, 2 for ≥5), and an identical final vowel, which keeps Hindi inflections like -te/-ta/-ti intact. Two-consonant words must be among the most common English words (SCOWL ≤20).
- **Protection:** a stoplist of about 400 common Hindi words, compared by phonetic key.

**Precision:** Deepgram writes English in Latin script, so every one of its Devanagari words is genuine Hindi. That makes it a natural negative set: 0 of 154 words are falsely converted.

**Known limits:**
- Ambiguous pairs without frequency data: let/late, bred/bread.
- Engine mis-hearings still produce English-looking errors.

**Rejected sources:**
- google-10000-english (non-commercial)
- FrequencyWords data (CC-BY-SA)

## ADR-012 — Speed first: the LLM is never in the blocking path
**Measured 2026-11-09** on the user's 40 clips, from India:

| step | release → text |
|---|---|
| Nemotron local finish + rules + translit/lexicon | **~160 ms** p50 / 171 ms p95 |
| Deepgram pre-recorded (HTTP upload after release) | 660 ms p50 / 915 ms p95 |
| Claude Haiku 5.5 cleanup (thinking off, effort low, cached prompt) | **+1043 ms** p50 / +1616 ms p95 |

The network is not the bottleneck: TCP takes 23 ms and TLS 50 ms to api.anthropic.com. Even a 5-token reply takes 0.8–1.3 s to the first byte, so the time is spent server-side.

**What cleanup is worth:** Nemotron Hinglish WER drops from 31.2% to 14.7% (CER from 14.1% to 8.2%). WER against verbatim references also counts intended edits as errors (self-corrections applied, "$1,250"), so English barely moves. One failure mode: a meta-reply ("I can't clean this…") on a garbled clip. Fixed with a prompt rule plus a length-ratio guard that falls back to Layer 1.

**Decision:** Layer 2 must never delay the first insertion.
- Default path: local streaming STT + rules + translit/lexicon, inserted in about 0.2 s.
- Layer 2 runs only where it can't be felt. Android composing text is replaced in place. Long dictations get cleanup per VAD segment while the user is still speaking. On desktop, cleanup is explicit opt-in per app.
- Cloud STT should move from pre-recorded to Deepgram **streaming** (WebSocket), so that recognition happens during speech. Needs a bench.
