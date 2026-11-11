# Benchmarks — plan & methodology

> **M1 final numbers** are in §9 and in the [M1 report](M1-REPORT.md).

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
- **Formatted reference `<name>.formatted.txt`** (optional): the text you would actually send, in the M2 style (ADR-0015). Numbers as digits (except one to nine in prose; money, times and dates always digits), fillers removed, self-corrections applied, spoken enumerations as a numbered list. A clip without one is scored against its verbatim `.txt` for formatted WER too, and `bolo` says how many clips lacked one.
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
| `summary.md` | one row per engine: WER, formatted WER and CER per category, p50/p95 latency, load time, peak RSS, download size, $/hr, and the count of Devanagari outputs |
| `summary.json` | the same data, machine-readable |
| `clips.csv` | per clip: latency, RTF, WER, formatted WER, CER, hypothesis vs both references |

Useful flags:

- `--lang by-folder|auto|en|hi`: the default `by-folder` sends `en` for `en/` and `auto` for everything else.
- `--threads N`
- `--no-cache`: costs money.
- `--polish`: also polish each output with Claude (needs `ANTHROPIC_API_KEY`) and add a Layer 1 + polish column for both WERs. It costs about $0.0013 per 40 clips. Without it no LLM calls are made.
- `--clips DIR`, `--out DIR`

`bolo replay` takes `--polish` too, and writes `replay.csv` and `summary.md` to `--out` (default `bench-results/replay`).

## 4. What each number means

| metric | definition |
|---|---|
| **WER** | Word edit distance divided by the number of reference words, summed over the corpus (not averaged per clip). Both sides are lowercased, punctuation is stripped, and aliases are applied. |
| **formatted WER** | WER against the formatted reference, the text you'd send. It measures how close the output is to finished text: fillers, self-corrections, list markers and spoken commands count, while digits vs number words don't (numbers are canonicalized on both sides, so "twelve fifty" = "$1,250"). Neither do capitals, punctuation or line breaks. Correct formatting edits lower it and raise verbatim WER. |
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

1. **Devanagari outputs.** Nemotron 3.5, Qwen3-ASR and Deepgram `multi` write Hindi words in Devanagari. `bolo-format` now transliterates them to Roman Hinglish and restores English loanwords (ADR-011), and the bench scores the transliterated output. Deepgram `multi` occasionally uses Urdu script instead. Since M2 ticket 07 that goes through the same path (§16).
2. **Sarvam `mode=translit` on `saaras:v4`.** The docs contradict each other. The first bench run will show whether the output is Roman.
3. **RSS isolation.** All engines run in one process, sequentially, and each local model is released after its pass. Allocator retention can inflate later engines' baselines. If the numbers look odd, run one engine per invocation.
4. **Streaming partials** are measured for cost (RTF) but not for quality. `bolo replay` measures release → text through the live engine (§7, §8), but not time to first partial.
5. **Not yet benchmarkable:** `qwen3-asr-hinglish` (Srota) and `whisper-base-hinglish` (Oriserve Swift). Neither has a sherpa-onnx export, and we will export both ourselves (`status: needs-export` in the manifest).

## 6. Decision rule (proposed)

- **Desktop default (local)**: the most accurate Hinglish engine with p50 < 500 ms and peak RSS < 1.5 GB.
- **Android ≥ 6 GB**: same rule, with p50 < 1.2 s on a mid-range phone (M5 re-runs the bench on-device).
- **Android 3–4 GB**: Moonshine-tiny for English and Whisper-base-Hinglish (once exported) for Hinglish.
- **Cloud default suggestion**: the lowest Hinglish WER among providers with p95 < 1.2 s.

## 7. Auto language mode (ADR-0014)

`bolo replay` drives the live engine. Each clip is played into it as if spoken, then the key is released:

```sh
$B replay --lang auto            # or en / hi; real time, ~5 min for 40 clips
$B replay --lang auto --speed 50 # WER and language choice only, ~100 s; latency is meaningless
```

**Shipping bar.** On the 40-clip set, Auto must reach English WER ≤ 8.5% and Hinglish WER ≤ 30%, with release→text p50 ≤ 180 ms. If it misses, Auto ships opt-in.

**Results (2026-11-09, `local:nemotron-stream`, Windows desktop, one real-time run per mode):**

| mode | WER en | WER hinglish | language chosen | stop→text p50 / p95 | engine-side p50 |
|---|---|---|---|---|---|
| `--lang en` | 8.0% | 83.0% | — | 243 / 4295* ms | 201 ms |
| `--lang hi` | 67.6% | 27.7% | — | 196 / 338 ms | 188 ms |
| `--lang auto` | **8.0%** | **27.7%** | 40 / 40 correct | 234 / 898 ms | 200 ms |

\* One outlier: a machine stall during that run.

**Latency columns.**
- *Stop→text* is measured by the caller, from `stop_dictation` to `on_final`. It includes waiting for the engine thread to finish a chunk decode that was in progress when the stop arrived. That wait is 0–190 ms and affects every mode equally.
- *Engine-side* is `Timings::total_ms`: from when the engine handles the stop to output ready.

Run-to-run noise on this machine is about ±40 ms p50; the same Auto build measured 205 ms and 275 ms in two earlier runs.

**Verdict: Auto is the default.**
- Accuracy is exactly that of the correctly forced language in both categories, because the detector never picked wrong.
- Latency matches the forced modes on both clocks. The 180 ms figure was calibrated on the `run` bench's finish-only timing (160–168 ms for Nemotron). On that measure Auto does the same work as a forced mode: when the Hindi pass's text at release already decides the language (40/40 clips), only the chosen pass is finished.
- No mode meets 180 ms on replay's end-to-end clock. That gap is the engine thread blocking on chunk decodes, not Auto, and is worth its own fix.
- Finishing both passes in parallel instead measured ~260 ms p50, which is why the early decision exists.

**Update (M2):** the chunk-decode wait is gone. Nemotron English is now 107 ms p50 and Auto 129 ms p50 on the same clock (§10).

**Tuned detector constants** (`bolo_format::language`), from the Hindi-pass output of all 40 clips:

| | English clips | Hinglish clips |
|---|---|---|
| Hindi function-word share | 0 (18 clips), 0.07 max | 0.25 min, 0.40 median |
| words | 4–21 | 3–18 |

- Hinglish if the share is ≥ 0.20.
- English if it is ≤ 0.10.
- Ambiguous if it is between those, or if there are fewer than 3 words. An ambiguous dictation reuses the previous dictation's language, or English if there is none.

## 8. Deepgram live streaming

`deepgram-stream` streams audio to Deepgram's live WebSocket while the user speaks and sends `Finalize` at release. Batch `deepgram` uploads at release instead (per VAD segment when Silero is installed).

```sh
$B replay -e deepgram-stream   # real time only: Deepgram takes live audio at ≤ 1.25× (replay refuses --speed > 1.25)
$B replay -e deepgram
```

**Results (2026-11-09, Nova-3, `--lang auto` → `language=multi`, Windows desktop in India, one real-time run each, both runs at the same time, 0 failures):**

| engine | WER en | WER hinglish | release→text p50 / p95 |
|---|---|---|---|
| `deepgram` (batch) | 7.6% | 22.8% | 1306 / 1907 ms |
| `deepgram-stream` | 9.2% | 21.0% | **299 / 370 ms** |

- Streaming cuts release→text by about 1 s at p50. What's left is about one network round trip plus Deepgram's flush of the last audio.
- Accuracy is within run-to-run noise of batch. Streaming fixes some batch errors (repeated words at segment joins, such as "Traffic Traffic" and "Bro Bro") and adds a few of its own.
- `multi` sometimes writes a Hindi word in Urdu script ("تیم" for "team"). This affected batch too. Fixed in M2 ticket 07 (§16).
- Neither cloud engine reaches the 180 ms local bar from India. Nemotron stays the default, and `deepgram-stream` is the cloud option for the instant path.

## 9. M1 final numbers

The numbers M1 ships on, gathered from §7, §8 and the `bolo run` results (2026-11-09, 40 clips, Windows desktop in India).

**Through the live engine (`bolo replay`, release → text, real time):**

| engine | mode | WER en | WER hinglish | p50 / p95 ms |
|---|---|---|---|---|
| `local:nemotron-stream` (default) | Auto | 8.0% | 27.7% | 234 / 898 (engine-side p50 200) |
| `local:nemotron-stream` | English | 8.0% | 83.0% | 243 / 4295* (engine-side p50 201) |
| `local:nemotron-stream` | Hinglish | 67.6% | 27.7% | 196 / 338 (engine-side p50 188) |
| `deepgram-stream` | Auto (`multi`) | 9.2% | 21.0% | 299 / 370 |
| `deepgram` (batch) | Auto (`multi`) | 7.6% | 22.8% | 1306 / 1907 |

\* One machine stall.

**Engine comparison (`bolo run`, Hinglish transliterated, `--lang by-folder`):**

| engine | WER en | WER hinglish | p50 / p95 ms | load ms | peak RSS MB | download MB |
|---|---|---|---|---|---|---|
| `local:nemotron-stream` | 8.0% | 31.2% | 158 / 171 | 2403 | 757 | 475 |
| `local:parakeet-v2-en` | 4.6% | 69.6% | 361 / 555 | 2346 | 795 | 482 |
| `local:qwen3-asr` | 5.3% | 32.1% | 1429 / 3599 | 3718 | 1688 | 879 |
| `local:moonshine-tiny` | 15.6% | 95.1% | 118 / 207 | 706 | 111 | 30 |
| `deepgram:nova-3` | 4.6% | 20.1% | 660 / 915 | – | – | – |

Here Nemotron's Hinglish uses its built-in language detection; Auto mode (above) brings it to 27.7%. Groq, Sarvam and `parakeet-v3` were not re-run after transliteration landed.

**Defaults chosen:** `local:nemotron-stream` in Auto on desktop (the only local engine with good Hinglish under 500 ms and 1.5 GB), `deepgram-stream` as the cloud option for the instant path, and LLM cleanup (`--polish`, +1043 ms p50, Hinglish WER 31.2% → 14.7%) only off the instant path (ADR-012).

## 10. Release → text without the chunk-decode wait (M2 ticket 01)

M1 missed the 180 ms bar on `bolo replay`'s end-to-end clock (§7). The stop breakdown, logged at `RUST_LOG=bolo_core=debug`, shows where the time went. The baseline below is `main`, 15 English clips, real time:

| stage | before | after (40 clips) |
|---|---|---|
| Stop waiting for the engine thread (an in-flight chunk decode) | 0–88 ms (0 on 2/3 of clips) | ~0.02 ms |
| last audio handed over | ~0.1 ms | ~0.01 ms |
| tail decode (in-flight chunk + padding chunks) | 150–186 ms: almost always 2 chunk decodes | p50 107, p95 237 ms: 0 decodes on 11 clips, 1 on 24, 2 on 5 |
| rules (romanize) | < 0.01 ms | < 0.1 ms |

Two changes:
1. **Each streaming pass decodes on its own thread.** The engine thread only hands audio over, so it takes Stop as soon as it's called. Auto drops the pass it didn't choose even mid-decode.
2. **The tail pad stops at the end of speech.** The fixed 0.6 s of silence before `input_finished` cost two chunk decodes after release, about 100 ms each. Now the stream pads only up to the chunk that holds the end of speech plus a 480 ms margin. It decodes nothing when that chunk was already decoded live, which is the common case when the user pauses before releasing.
   - The end of speech is the last 10 ms frame within −46 dB of the dictation's loudest frame. In a noisy room every frame counts, which costs latency, not words.
   - The chunk geometry is probed once at model load by feeding silence. On Nemotron 320 ms, the first chunk is ready at 6848 samples, then one every 5120.

**Tuning (40 clips, `--speed 50`).** WER with 0 ms and 300 ms of silence before release, against the old fixed pad:

| tail rule | en, 0 ms | en clips forced to hi, 0 ms | en clips forced to hi, 300 ms |
|---|---|---|---|
| fixed 0.6 s (old) | 8.0% | 67.6% | 67.6% |
| speech end at −26 dB + 160 ms | 9.2% | 68.3% | 67.9% |
| speech end at −46 dB + 160 ms | 8.4% | 68.3% | 67.9% |
| speech end at −46 dB + 320 ms | 8.0% | – | 67.9% |
| **speech end at −46 dB + 480 ms** | **8.0%** | **67.6%** | **67.6%** |

Shorter margins cut final syllables ("shopping li", "cache la"): the transducer emits a word's last tokens a while after hearing them. Hinglish clips stayed at 27.7% in every row. With 480 ms, every cell in both categories and both modes equals the old fixed pad.

**Results (2026-11-10, `local:nemotron-stream`, Windows desktop, real time, 40 clips; `main` and this change run back to back):**

| mode | build | WER en | WER hinglish | release→text p50 / p95 |
|---|---|---|---|---|
| English | `main` | 8.0% | 83.0% | 215 / 313 ms |
| English | this change | 8.0% | 83.0% | **107 / 220 ms** ✅ |
| Auto | `main` | 8.0% | 27.7% | 229 / 350 ms |
| Auto | this change | 8.0% | 27.7% | **129 / 267 ms** ✅ |

- Auto still chooses the right language on 40/40 clips.
- `Timings` now run from the `stop_dictation` call, so the engine-side and caller-side numbers agree.
- Inference threads were also tried. 4 threads was fastest: 8 and 2 were both slower per chunk.
- `bolo run`'s finish-only latency also drops, since it uses the same tail rule. That table (§9) hasn't been re-run.
- p95 is still above 180 ms. It comes from releases mid-speech, which need 2 tail decodes.

## 11. Formatted references and the formatted-WER baseline (M2 ticket 02)

Each clip now has a formatted reference (`<name>.formatted.txt`, §1). The drafts differ from the verbatim reference on 7 of 40 clips:

| clip | formatted reference | what changed |
|---|---|---|
| en/04 | I'll be 10 minutes late, start without me. | number style |
| en/06 | The invoice number is 4572 and the total comes to $1,250. | money, digits |
| en/11 | 1. Update the dependencies. ⏎ 2. Run the tests. ⏎ 3. Deploy to staging. | spoken list |
| en/12 | Let's meet on Wednesday after lunch. | self-correction |
| en/17 | My flight lands at 6:30 in the evening, so I can join the call from the airport. | time |
| hinglish/11 | Parson chalte hain. | self-correction ("nahi wait") |
| hinglish/15 | Aaj traffic bahut zyada hai, main 15 minute late ho jaunga. | Hindi number word (left to polish by ADR-0015) |

Number style barely moves WER, since numbers are canonicalized on both sides: en/04 and en/17 score the same either way, and en/06 differs by one word ("$1,250" has no "dollars"). The recordings have no fillers, so filler removal isn't measured by this set.

**Baseline before Layer 1 rules (2026-11-10, 40 clips):**

| run | engine | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|---|
| `bolo run` | `local:nemotron-stream` | 8.0% | 11.3% | 31.2% | 34.6% |
| `bolo run` | `deepgram:nova-3` (cached) | 4.6% | 7.4% | 20.1% | 24.4% |
| `bolo replay --speed 50`, Auto | `local:nemotron-stream` | 8.0% | 11.3% | 27.7% | 30.9% |
| same, `--polish` | `local:nemotron-stream` + polish | 9.5% | **8.2%** | 15.6% | 18.4% |

- Polish lowers formatted WER on English (11.3% → 8.2%) while it raises verbatim WER (8.0% → 9.5%): it makes the edits a person would make. That's the gap Layer 1 aims to close on the instant path.
- The formatted references are drafts until the user signs them off (ticket 02). Numbers here will be re-run if they change.

## 12. Layer 1 rules: punctuation fix-ups and commands (M2 ticket 03)

The rules pipeline (`bolo_format::rules`, ADR-0015) now runs on every final, inside `rules_ms`. So far it has the punctuation fix-ups, the "new line" and "new paragraph" commands, and Plain/`code_mode` rendering. `bolo run` runs it on each transcript (after romanization) before scoring and before `--polish`. `bolo replay` gets it through the engine.

**Formatted WER after this ticket (2026-11-10, 40 clips):**

| run | engine | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|---|
| `bolo run` | `local:nemotron-stream` | 8.0% | 11.3% | 31.2% | 34.6% |
| `bolo run` | `deepgram:nova-3` (cached) | 4.6% | 7.4% | 20.1% | 24.4% |
| `bolo replay --speed 50`, Auto | `local:nemotron-stream` | 8.0% | 11.3% | 27.7% | 30.9% |

- Every number is unchanged from §11, as expected. WER ignores case and punctuation, which is all these passes touch, and no clip says "new line". The visible change is in the text: unpunctuated engine output now gets a capital and a final period ("kal meeting hai na achenda bhej dena" → "Kal meeting hai na achenda bhej dena."). The gap to the formatted references starts closing with fillers, corrections, numbers and lists (tickets 04–06).
- **Latency gate:** `crates/bolo-format/tests/latency.rs` runs the whole pipeline over the 40 clips' transcripts, as written and as bare lowercase text, in all three format contexts, and fails above 1 ms p95. It measured p95 0.027 ms in a debug build, so it has a wide margin on CI runners.

## 13. Fillers and self-corrections (M2 ticket 04)

The rules pipeline now removes fillers and applies spoken self-corrections (ADR-0015), ahead of the commands and punctuation passes.

- **Fillers.** The pass removes hesitations ("um", "uh", "er"), "you know" between pauses, "like" between commas, and stutters ("the the", "I, I think"). Hindi fillers ("matlab", "haan toh", "woh", "na") go only at the start of a Hinglish dictation or as "matlab matlab". Words repeated on purpose are kept, as are numbers and Hindi reduplication ("jaldi jaldi").
- **Self-corrections.** These act on "actually no", "I mean", "sorry", "scratch that", "nahi nahi" and "nahi wait". The words after the marker replace the nearest preceding date, time, number or (English only) noun phrase. They also replace the clause when the repair restates it.

**Formatted WER after this ticket (2026-11-10, 40 clips):**

| run | engine | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|---|
| `bolo run` | `local:nemotron-stream` | 9.9% | **9.4%** | 31.2% | 34.6% |
| `bolo run` | `deepgram:nova-3` (cached) | 6.5% | **5.5%** | 23.2% | **21.2%** |
| `bolo replay --speed 50`, Auto | `local:nemotron-stream` | 9.9% | **9.4%** | 27.7% | 30.9% |

- Exactly three outputs changed, and each now matches its formatted reference word for word:
  - en/12 on both engines. Nemotron writes "Let's meet on Tuesday actually no make it Wednesday after lunch." with no commas. Deepgram writes "… Tuesday. Actually, no. Make it …", with the marker in a sentence of its own.
  - hinglish/11 on Deepgram ("Kal shaam ko chalte hain. Nahin wait parson chalte hain." → "Parson chalte hain.").

  Verbatim WER rises on those clips by design: the output is now what the user meant, not what they said. Formatted WER on English drops 11.3% → 9.4% (Nemotron) and 7.4% → 5.5% (Deepgram), which is past Nemotron + polish's 8.2% in §11.
- Nemotron's hinglish/11 ("Par shaam ko chalta hai nahin wait parson chalte hain.") stays verbatim. The recognizer heard "Par", not "kal", so there's no date for "parson" to replace. Polish can still fix it.
- The recordings have no fillers, so this set doesn't measure the filler pass. The golden cases and property tests cover it.
- **Latency gate:** p95 0.065 ms in a debug build (was 0.027 ms), still 15× under the 1 ms budget.

## 14. Numbers (M2 ticket 05)

The rules pipeline now writes English number words in the ADR-0015 number style. The pass runs after self-corrections:
- Cardinals become digits, except one through nine in prose.
- Money, times, dates and percentages always use digits.
- Grouping is Indian when lakh, crore or rupees is spoken.
- Four or more digits spoken one by one join up ("four five seven two" → "4572").
- Number words that don't read as one number, with nothing in the context to settle them ("twenty twenty six", "ten fifteen"), stay verbatim, as do Hindi number words.

**Formatted WER after this ticket (2026-11-10, 40 clips):**

| run | engine | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|---|
| `bolo run` | `local:nemotron-stream` | 10.3% | **9.0%** | 31.2% | 34.6% |
| `bolo run` | `deepgram:nova-3` (cached) | 6.5% | 5.5% | 23.2% | 21.2% |
| `bolo replay --speed 50`, Auto | `local:nemotron-stream` | 10.3% | **9.0%** | 27.7% | 30.9% |

- Nemotron writes numbers as words, and all four of its number clips now come out in style:
  - en/04: "10 minutes"
  - en/05: "at 4 PM"
  - en/06: "4572" and "$1,250"
  - en/17: "at 6:30 in the evening"

  Formatted English WER drops 9.4% → 9.0%. What's left on those clips is recognition error ("This book a meeting", "My flights land"). Verbatim WER rises, by design, because the verbatim references keep "ten minutes".
- Deepgram already writes digits ("$1,250", "4PM", "06:30"), and the pass leaves numerals the engine wrote as they are, so nothing changed.
- Hinglish is unchanged. Its only number in the set is hinglish/15's "pandrah" (Hindi), which is left for polish.
- **Latency gate:** p95 0.12 ms in a debug build (was 0.065 ms), still 8× under the 1 ms budget.

## 15. Lists and Markdown output (M2 ticket 06)

The rules pipeline now turns an explicit spoken enumeration of three or more items into a numbered list (ADR-0015). The enumeration can be "first … second … third", "one … two … three" (each at a clause start), or "number one … number two …". Words before the first marker in the same sentence become the intro line, ending in a colon. Prose joined with "and", two-item enumerations, "the first …", "first of all" and quantities ("one apple, two pears") stay prose. The engine fills `FormattedOutput.plain` and `.markdown` from one pipeline run. Markdown adds a blank line before and after a list. The lists pass now runs before the numbers pass, so removing a marker can't change how the numbers around it are read (ADR-0015 amended).

**Formatted WER after this ticket (2026-11-10, 40 clips):**

| run | engine | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|---|
| `bolo run` | `local:nemotron-stream` | 10.3% | 9.0% | 31.2% | 34.6% |
| `bolo run` | `deepgram:nova-3` (cached) | 7.6% | **4.3%** | 23.2% | 21.2% |
| `bolo replay --speed 50`, Auto | `local:nemotron-stream` | 10.3% | 9.0% | 27.7% | 30.9% |

- en/11 is the set's only list. Deepgram writes "First, update the dependencies. Second, run the test. Third, deploy to staging.", and that now comes out as the formatted reference's numbered list. The only remaining error is "test" for "tests" (fmt WER 33% → 8%). Formatted English WER drops 5.5% → 4.3%. Verbatim WER rises 6.5% → 7.6% by design, because the markers are gone.
- Nemotron doesn't hear the opening "First" ("Update the dependencies second run the test third deploy to staging"). With only two markers the text correctly stays prose, so Nemotron's numbers don't change.
- **Latency gate:** p95 0.14 ms in a debug build (was 0.12 ms) and 0.03 ms in release, still 7× under the 1 ms budget.

## 16. Urdu script to Roman Hinglish (M2 ticket 07)

`bolo_format::urdu` maps Urdu script to Devanagari, and the result goes through the existing romanizer and loanword lexicon. This happens everywhere Roman-script mode romanizes Devanagari: in the engine (`HinglishScript::Roman`), in the `bolo run` and cleanup scoring, and in the Hinglish detector's function-word count. Because Urdu leaves short vowels unwritten, the map is a best guess, built in three parts:
- a whole-word table for about 90 frequent Hindi words (ہے → है → "hai", کیا → "kya", بھیج → "bhej");
- a letter map for everything else (ھ aspirates, ں nasalizes, و and ی become vowels after a consonant);
- two narrow heuristics for English loanwords: final consonant clusters (ٹیسٹ → "test", دوست → "dost") and a final "-ing" (میٹنگ → "meeting").

Unit tests cover the observed case (تیم → "team"), plus ٹیم, میٹنگ, رپورٹ, پروجیکٹ, ڈاکٹر and 28 common function words. A property test checks that Roman-script output never contains an Arabic-script character. Characters the map doesn't know are dropped, never passed through.

**Re-measured (2026-11-10, `bolo replay -e deepgram-stream --speed 1.25`, 40 clips, `language=multi`, 0 failures):**

| engine | WER en | fmt WER en | WER hinglish | fmt WER hinglish | stop→text p50 / p95 |
|---|---|---|---|---|---|
| `deepgram-stream` | 12.2% | 10.9% | 22.8% | 26.3% | 290 / 372 ms |

- Deepgram emitted no Urdu script in this run, so the WER doesn't move because of this ticket. hinglish/14, where "تیم" appeared in §8, came back in Latin ("Team is week ka …"). The cached batch outputs have no Urdu script either. The map is covered by unit and property tests rather than by this sample. Hinglish WER is within live-run noise of §8 (21.0%). The English drop from 9.2% to 12.2% comes from new live misses (en/12, en/13, en/14), not from this change, which leaves Latin text untouched.
- **New gap:** hinglish/09 ("Ghar ke liye doodh, bread, ande aur sabzi le aana") came back entirely in **Telugu script** (100% WER). `multi` can pick a third script, so a general fix belongs with script handling, not with this map. It's logged as a follow-up.


## 17. Per-segment polish on long dictations (M2 ticket 09)

With polish on, a dictation that reaches 20 s is polished one segment at a time while it is still being spoken (ADR-0016). Each pause of 700 ms or more closes a segment. At release only the rest is polished, then the parts are joined.

**How it was measured:** `bolo polish-latency` (2026-11-10). The English clips were chained with 1 s pauses into five dictations of 61–67 s, 106–119 words each. They were played in real time into `local:nemotron-stream` (English), with polish on and live `claude-haiku-5-5`. "Whole text" is release → `on_final` plus one polish call on the same instant text, which is how the dictation would be polished without segments.

| run | audio | release → `on_final` | release → `on_polished`, per segment | release → polished, whole text |
|---|---|---|---|---|
| 1 | 67.4 s | 76 ms | 128 ms | 1496 ms |
| 2 | 61.2 s | 1 ms | 2 ms | 1385 ms |
| 3 | 63.2 s | 153 ms | 402 ms | 2127 ms |
| 4 | 65.5 s | 143 ms | 143 ms | 1572 ms |
| 5 | 67.2 s | 1 ms | 2 ms | 1487 ms |
| **p50** | | **76 ms** | **128 ms** | **1496 ms** |

- **Per segment, `on_polished` arrives 128 ms p50 after release**, about 50 ms after the instant text. The whole-text polish of 60 s of speech takes 1.4–2.1 s. That is at or over the engine's 1.5 s deadline, so without segments three of the five dictations would have **lost their polish entirely**: runs 1 and 5 came in just under the deadline, and runs 3 and 4 went over it.
- Each dictation was cut into 5–7 segments (33 in total). All five runs were polished.
- **Caveat:** each chained dictation ends with its 1 s clip pause plus 300 ms before release. That closes the last segment before release, so in runs 2, 4 and 5 the remaining text was empty and `on_polished` came straight after `on_final`. Someone who releases mid-sentence waits for one short polish call on the last phrase instead: runs 1 and 3 show +50 to +250 ms. That stays far under a whole-text call.
- One segment call failed with a network error (run 5, first segment). It kept its Layer 1 text, and the rest of that dictation was still polished and joined. This is the fallback the engine tests cover.
- Release → `on_final` of 1 ms (runs 2 and 5): after a long trailing pause the decoder has nothing left to decode at release.

## 18. M2 final numbers

These are the numbers M2 ships on: `main` at `e087099`, measured 2026-11-10 on 40 clips. The M2 report ([M2-REPORT.md](M2-REPORT.md)) has the discussion.

**Through the live engine (`bolo replay`, `local:nemotron-stream`, Auto):**

| layer | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|
| Layer 1 | 10.3% | 9.0% | 27.7% | 30.9% |
| Layer 1 + polish | 9.9% | 8.6% | 12.9% | 15.7% |

These are from `--speed 50 --polish` (40 polish calls, 0 failed, $0.0012). Real-time release → text: Auto 182 / 375 ms p50 / p95 (two runs, 183 and 182 ms p50), English 151 / 297 ms (20 clips). The ticket 01 build, run the same day, reads Auto 190 / 377 ms, so the machine was slower than in §10, and tickets 03–09 added no latency.

**`bolo run --polish`:**

| engine | WER en | fmt WER en | +polish fmt WER en | WER hinglish | fmt WER hinglish | +polish fmt WER hinglish | p50 / p95 ms |
|---|---|---|---|---|---|---|---|
| `local:nemotron-stream` | 10.3% | 9.0% | 8.6% | 31.2% | 34.6% | 20.3% | 285 / 455 |
| `deepgram:nova-3` (cached) | 7.6% | 4.3% | 3.5% | 23.2% | 21.2% | 13.8% | 660 / 915 |

80 polish calls, 1 failed (its Layer 1 text was kept), $0.0026. Nemotron's finish-only p50 is 285–295 ms over three runs. The ticket 01 build reads 286 ms. M1's 158 ms (§9) hasn't been reproduced, and the cause hasn't been investigated (M2 report, gap 3).

**Rules latency** (`cargo test -p bolo-format --release --test latency`): p95 0.025 ms, max 0.23 ms over 4800 runs.

## 19. Other Indic scripts to Roman Hinglish (M3 ticket 12)

`bolo_format::brahmic` maps Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada and Malayalam script to Devanagari. The result then goes through the same romanizer and loanword lexicon as Devanagari and Urdu (§16). This happens everywhere `translit::needs_romanizing` gates Roman-script mode: the engine, `bolo run` and cleanup scoring, and the Hinglish detector.
- Unicode lays these eight blocks out like Devanagari, so most letters map by their offset within the block. A letter maps only when the Devanagari codepoint at that offset is one the romanizer reads.
- A small explicit table covers the letters that don't map that way: Dravidian short e and o (read as e and o), ற/ఱ/റ (r), ழ/ఴ/ೞ/ഴ (ḷ), Tamil ன, Malayalam chillu letters and dot reph, Bengali ৎ, Assamese ৰ/ৱ, Gurmukhi tippi (nasal) and addak (doubles the next consonant: ਪੱਕਾ → "pakka").
- Letters at the Devanagari nukta-letter offsets (U+0958–095F) map by offset only in Bengali, Gurmukhi and Oriya. In the other scripts those slots hold different letters (Telugu ౘ, ౙ) or Malayalam fraction signs. Decomposed vowel signs (Tamil ெ + ா, Telugu ె + ౖ, Kannada ಿ + ೕ) are folded back into one sign.
- Anything else in those blocks is dropped, never passed through.

**Tests:** golden cases give "ghar ke liye doodh le aana" in all eight scripts. Tamil, which has no voiced or aspirated stops, gives "kar ke liye tooth le ana". There are also loanword cases (మీటింగ్ → "meeting", ઓફિસ → "office") and the hinglish/09 Deepgram transcript as a regression case. A property test checks that Roman-script output contains no Brahmic or Arabic-script character.

**Re-measured (2026-11-11):**
- `bolo run --engines deepgram:nova-3`, from the cached `multi` outputs: hinglish/09's Telugu output now comes out as "Karike le noon bred andi parsabji lehaal." instead of Telugu script. Deepgram misheard most of the words, so its WER stays at 100%, but its CER drops from 100% to 42%. Hinglish CER drops from 13.0% to 10.7%. Every other number is unchanged: WER en 7.6%, fmt WER en 4.3%, WER hinglish 23.2%, fmt WER hinglish 21.2%.
- `bolo replay -e deepgram --speed 50`, live `multi`: no Brahmic or Urdu script appeared in any of the 40 outputs. This time hinglish/09 came back in Latin script ("Rose, bread, ande aur sabzi lehana."). WER en was 17.9% and WER hinglish 33.0%, but these aren't comparable with the real-time batch figures (7.6% / 22.8%, §8). Several outputs lose their first words at 50× ("Tak bhej dena…" for "Report Friday tak bhej dena…"). That looks like a replay artifact of the batch engine at high speed, not a script problem. It's left for follow-up.

## 20. `bolo run`'s Nemotron finish-only latency (M3 ticket 13)

M2 measured `bolo run --engines local:nemotron-stream` at 285–295 ms finish-only p50, against 158 ms in M1 (§9, §18). To find out whether a code change caused it, three builds ran back to back, one at a time, on 2026-11-11 on the same machine and the same 40 clips (`--runs 3`, median per clip):

| build | commit | p50 / p95 ms | WER en | WER hinglish |
|---|---|---|---|---|
| M1 release `v0.1.0` | `c8276d0` | 226 / 255 | 8.0% | 31.2% |
| M2 ticket 01 (decoder threads, tail pad) | `465954d` | 228 / 349 | 8.0% | 31.2% |
| M2 final | `e087099` | 235 / 381 | 10.3%* | 31.2% |

\* The M2 build scores the rules-formatted text (§12 onwards); its raw WER is unchanged.

**Finding: no code change caused it. The machine is slower.** The M1 binary itself reads 226 ms today, not 158 ms. The three builds sit within 9 ms of each other, inside run-to-run noise. The 285–295 ms read in M2 came from a busier or throttled machine that day (the same day's `bolo replay` also read slower; see §18). This laptop's `bolo run` timings drift by tens of milliseconds with what else runs on it, so compare builds only in back-to-back runs like these. Machine load was not controlled: other apps were open during these runs too.

No fix is needed. The user-facing clock, `bolo replay`'s release → text, is unaffected by this question.
