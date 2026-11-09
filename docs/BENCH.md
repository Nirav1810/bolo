# Benchmarks — plan & methodology

The benchmark subcommands of the `bolo` CLI decide which engines ship as defaults. They were built first and run on **your** voice, mic, and accent. Public benchmarks don't measure Hinglish dictation.

## 1. Record the clips (you)

```
testdata/clips/
  en/        01-email-followup.wav   01-email-followup.txt   …  (~20)
  hinglish/  01-meeting-kal.wav      01-meeting-kal.txt      …  (~20)
  aliases.tsv                       # optional: accepted spelling variants
```

- **Audio**: any WAV (16/24/32-bit int or float, any rate, mono or stereo). It is down-mixed and resampled to 16 kHz.
- Record with the mic you will actually dictate with, in your normal environment.
- **Length mix**: about 40% short (2–5 s: "send it", "ok kal milte hain"), 40% medium (5–15 s), 20% long (15–30 s).
- **Content**: real dictation. Emails, chat replies, lists ("first… second…"), corrections ("actually no, make it Friday"), names and product terms, numbers, and code identifiers.
- **Reference `.txt`**: exactly what you said, with fillers removed. Write Hinglish in **Roman script, the way you would type it** (`kal meeting hai na, agenda bhej dena`). Punctuation and case don't matter, because both sides are normalized.
- **`aliases.tsv`**: `variant<TAB>canonical`, one per line (`nahin	nahi`, `hain	hai`). Hinglish has no standard spelling. Without aliases, WER penalizes valid spellings.
- Audio is gitignored. Only the `.txt` references and `aliases.tsv` are committed, if you choose to commit them.

## 2. Install engines

```sh
python -I scripts/fetch-sherpa-libs.py         # once per machine (verified prebuilt libs)
cargo build --release -p bolo
B=target/release/bolo

$B models                                       # what's available / installed
$B download nemotron-stream parakeet-v2-en parakeet-v3 qwen3-asr moonshine-tiny silero-vad
#   ≈ 2.9 GB total; SHA-256 verified; resumable (re-run after an interruption)
$B sysinfo                                      # RAM + recommended default
```

Cloud keys are read from the environment only and never written to disk:

```sh
export GROQ_API_KEY=… DEEPGRAM_API_KEY=… SARVAM_API_KEY=…
```

## 3. Run

```sh
$B run --engines local:nemotron-stream,local:nemotron-stream-560,local:parakeet-v2-en,local:parakeet-v3,\
local:qwen3-asr,local:moonshine-tiny,groq,deepgram,sarvam \
       --dictionary "Bolo,Tauri,Kotlin" --runs 3
```

Output goes to `bench-results/`:

| file | contents |
|---|---|
| `summary.md` | one row per engine: WER/CER per category, p50/p95 latency, load time, peak RSS, download size, $/hr, and the count of Devanagari outputs |
| `summary.json` | the same data, machine-readable |
| `clips.csv` | per clip: latency, RTF, WER, CER, hypothesis vs reference |

Useful flags:

- `--lang by-folder|auto|en|hi`: the default `by-folder` sends `en` for `en/` and `auto` for everything else.
- `--threads N`
- `--no-cache`: costs money.
- `--clips DIR`, `--out DIR`

## 4. What each number means

| metric | definition |
|---|---|
| **WER** | Word edit distance divided by the number of reference words, summed over the corpus (not averaged per clip). Both sides are lowercased, punctuation is stripped, and aliases are applied. |
| **CER** | The same at character level with spaces ignored. It is more forgiving of Hinglish spelling variants. |
| **latency (local)** | *Release → text.* Streaming engines (Nemotron) are fed in 100 ms chunks first, which mirrors decoding while you speak. Only `finish()` is timed: 0.6 s tail padding plus a flush. Batch engines time the full decode. The median over `--runs` is taken per clip, then p50/p95 across clips. |
| **latency (cloud)** | Request round-trip on a **pre-warmed** connection (a HEAD request first, as the app does on hotkey-down). It includes WAV upload. It is measured once, then cached. |
| **RTF** | latency ÷ audio duration (in `clips.csv`). |
| **load ms** | Cold `OfflineRecognizer`/`OnlineRecognizer::create`. The OS file cache may already be warm. |
| **peak RSS MB** | Process RSS sampled every 10 ms, peak minus the baseline before load. It covers the model plus inference buffers. |
| **$/hr** | List price × billed duration ÷ real duration. Groq bills ≥10 s per request, so short clips cost up to 5× list. Deepgram is priced as multilingual, plus keyterm when a dictionary is set. Sarvam ₹0.49/min ≈ $0.35/hr. |
| **Devanagari** | Number of outputs containing Devanagari. These score badly against Roman references until transliteration lands (see below). |

### Cloud cache

Responses are cached in `bench-cache/<engine>/<sha256(audio ‖ options)>.json` together with the latency measured on the first call. A rerun costs nothing and reports that first latency. Use `--no-cache` to re-measure network latency.

## 5. Known limitations and next steps

1. **Devanagari outputs.** Nemotron 3.5, Qwen3-ASR and (likely) Deepgram `multi` write Hindi words in Devanagari. The plan is a Devanagari → Roman Hinglish transliterator in `bolo-format`, with schwa deletion and common-spelling tables such as है→hai and नहीं→nahi. The bench will score both raw and transliterated output. Until then, read Hinglish **CER** for those engines with care.
2. **Sarvam `mode=translit` on `saaras:v4`.** The docs contradict each other. The first bench run will show whether the output is Roman.
3. **RSS isolation.** All engines run in one process, sequentially, and each local model is released after its pass. Allocator retention can inflate later engines' baselines. If the numbers look odd, run one engine per invocation.
4. **Streaming partials** are measured for cost (RTF) but not for quality. A "time to first partial" metric comes with capture in M1 phase 2.
5. **Not yet benchmarkable:** `qwen3-asr-hinglish` (Srota) and `whisper-base-hinglish` (Oriserve Swift). Neither has a sherpa-onnx export, and we will export both ourselves (`status: needs-export` in the manifest).

## 6. Decision rule (proposed)

- **Desktop default (local)**: the most accurate Hinglish engine with p50 < 500 ms and peak RSS < 1.5 GB.
- **Android ≥ 6 GB**: same rule, with p50 < 1.2 s on a mid-range phone (M5 re-runs the bench on-device).
- **Android 3–4 GB**: Moonshine-tiny for English and Whisper-base-Hinglish (once exported) for Hinglish.
- **Cloud default suggestion**: the lowest Hinglish WER among providers with p95 < 1.2 s.
