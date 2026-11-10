# M2 report: the formatting layer

**Date:** 2026-11-10. **Release:** `v0.2.0` (Windows x64 CLI preview, prerelease: unsigned, ADR-0013).

M2 turns raw recognition into text you'd send. Layer 1 rules run on every dictation in well under a millisecond: they remove fillers, apply spoken self-corrections, and handle numbers, lists, line-break commands and punctuation. Polish (Layer 2, Claude) moved into the engine as a second event that never delays the instant text. Long dictations are polished segment by segment while the user is still talking. Stop no longer waits on a chunk decode, and Urdu-script words are romanized. The `bolo` CLI is still the only product. The desktop app starts in M3.

## What shipped

| area | what | where |
|---|---|---|
| Stop path | Each streaming pass decodes on its own thread, so Stop is handled at once. The tail pad ends 480 ms after the end of speech instead of a fixed 0.6 s, which usually means zero or one tail decode instead of two. | ticket 01, BENCH.md §10 |
| Layer 1 rules | `bolo_format::rules`: fillers → self-corrections → commands and lists → numbers → punctuation fix-ups → render (Plain, Markdown, `code_mode`). Conservative: when unsure, the text stays verbatim. Golden TOML cases per pass, property tests (idempotent, never empty, never drops a non-filler word) and a latency gate in CI. | ADR-0015, tickets 03–06 |
| Number style | Digits except one through nine in prose; money, times, dates and percentages always in digits; Indian grouping when lakh, crore or rupees is spoken ("₹1,25,000"). | ADR-0015, ticket 05 |
| Lists | An explicit enumeration of three or more items ("first … second … third") becomes a numbered list. In Markdown it gets blank lines around it. Prose joined with "and" stays prose. | ticket 06 |
| Urdu script | `bolo_format::urdu` maps Urdu script to Devanagari ahead of the romanizer and loanword lexicon (تیم → "team"). Roman-script output never contains Arabic script. | ticket 07 |
| Polish in the engine | `EngineConfig::polish` (`Off` by default, or `On { tone, app, dictionary }`). After `on_final`, the engine calls Claude and emits `on_polished`, or nothing when polish times out (1.5 s), fails or fails its guard. The next `start_dictation` or `cancel_dictation` supersedes a running polish. `LlmClient` is the test seam. `bolo dictate --polish` now uses the event. | ADR-0016, ticket 08 |
| Per-segment polish | With polish on, a dictation that reaches 20 s is cut at pauses of 700 ms or more (Silero VAD, or an energy gate without the model). Each segment is polished in the background with the previous one as context, and only the rest is polished at release. | ADR-0016, ticket 09 |
| Bench | Formatted references (`*.formatted.txt`) for the 40 clips. `bolo run` and `bolo replay` report formatted WER next to verbatim WER, and a Layer 1 + polish column with `--polish`. `bolo polish-latency` measures per-segment polish on chained 60 s dictations. | ticket 02, BENCH.md §11, §17 |

## Accuracy

40 clips (20 English, 20 Hinglish), one speaker, one mic. Methodology: [BENCH.md](BENCH.md). **Verbatim WER** compares with what was said. **Formatted WER** compares with what the user would send (numbers in style, corrections applied, lists as lists), so it is the number M2 is about. Run 2026-11-10 on `main` at `e087099`.

### Through the live engine (`bolo replay --speed 50 --polish`, Auto)

| `local:nemotron-stream`, Auto | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|
| M1 (no rules) | 8.0% | 11.3% | 27.7% | 30.9% |
| **Layer 1** | 10.3% | **9.0%** | 27.7% | 30.9% |
| **Layer 1 + polish** | 9.9% | **8.6%** | 12.9% | **15.7%** |

Auto chose the right language on 40 of 40 clips.

### Engine comparison (`bolo run --polish`)

| engine | layer | WER en | fmt WER en | WER hinglish | fmt WER hinglish |
|---|---|---|---|---|---|
| `local:nemotron-stream` | Layer 1 | 10.3% | 9.0% | 31.2% | 34.6% |
| `local:nemotron-stream` | Layer 1 + polish | 9.9% | 8.6% | 16.5% | 20.3% |
| `deepgram:nova-3` (cached) | Layer 1 | 7.6% | **4.3%** | 23.2% | 21.2% |
| `deepgram:nova-3` (cached) | Layer 1 + polish | 6.9% | **3.5%** | 17.0% | **13.8%** |

(`bolo run`'s Nemotron Hinglish uses the engine's own language detection, not Auto. That's why it's worse than the replay table.)

- **Layer 1 closes most of the English formatting gap on the instant path.** Formatted English WER fell from 11.3% to 9.0% on Nemotron and from 7.4% to 4.3% on Deepgram, with no LLM. Before M2, only polish reached 8.2% (BENCH.md §11).
- **Verbatim WER rises by design.** For example, "Tuesday actually no make it Wednesday" now comes out as "Wednesday", and "ten minutes" as "10 minutes". The output is what the user meant, not what they said.
- **Hinglish formatting is still polish's job.** Layer 1 doesn't change the Hinglish clips. Their remaining errors are recognition and Roman spelling, plus a Hindi number word ("pandrah"), which ADR-0015 leaves to polish. Polish halves Hinglish formatted WER: 30.9% → 15.7% on Nemotron Auto.
- Each pass's own effect is in BENCH.md §12–§16.

## Latency

### Release → text (real-time `bolo replay`, 40 clips, `local:nemotron-stream`)

| mode | M1 (BENCH.md §9) | M2 ticket 01 (§10) | M2 final, today |
|---|---|---|---|
| English | 243 / 4295* ms | **107 / 220 ms** | 151 / 297 ms (20 clips) |
| Auto (default) | 234 / 898 ms | **129 / 267 ms** | 182 / 375 ms |

p50 / p95. \* One machine stall in M1.

- **The ticket 01 gate held:** Nemotron English p50 fell to 107 ms, against the 180 ms bar.
- **Today the machine is slower.** For a same-day A/B, the ticket 01 build was run back to back with the final build: Auto 190 ms (ticket 01) vs 182 ms (final) p50. Tickets 03–09 added no latency. The difference from §10 comes from the machine, not the code: Auto lands 2 ms over the bar today and English is under it.
- **p95 is still over 180 ms.** It comes from releases in the middle of a word, which need two tail decodes (§10).

### Layer 1 rules

`crates/bolo-format/tests/latency.rs` runs the whole pipeline over the 40 clips' transcripts in all three format contexts (4800 runs). **p95 0.025 ms, max 0.23 ms in a release build**, which is 40× under the 1 ms budget. CI enforces < 1 ms p95 in a debug build.

### Polish

| | release → polished text |
|---|---|
| Whole-text polish, short clips (M1, `--polish`) | +1043 ms p50 after the instant text |
| Whole-text polish, 60 s dictations | 1496 ms p50 (1385–2127 ms). 2 of 5 runs miss the 1.5 s deadline and lose their polish, and 2 more come within 15 ms of it. |
| **Per-segment polish, 60 s dictations** | **128 ms p50** (2–402 ms), all 5 polished |

Measured with `bolo polish-latency`: five chained English dictations of 61–67 s, live `claude-haiku-5-5` (BENCH.md §17). The instant text (`on_final`) always comes first.

## Known gaps

1. **Owed by the human:**
   - sign-off on the drafted formatted references (ticket 02; hinglish/11 and hinglish/15 are flagged);
   - a live `bolo dictate --polish` try (ticket 08);
   - from M1, the `bolo dictate` speech check (M1 ticket 05) and the clean-profile zip run (M1 ticket 07).
2. **Release → text p50 sits near the bar.** It's under 180 ms on a quiet machine (§10) and about 180 ms today. p95 is 220–375 ms when the release comes mid-word.
3. **`bolo run`'s finish-only latency for Nemotron reads 285–295 ms p50** (M1: 158 ms). The ticket 01 build reads 286 ms today, so tickets 03–09 didn't cause it. Ticket 01 or the machine might have. It hasn't been investigated, because `bolo replay` is the clock users feel.
4. **Deepgram `multi` can write a third script.** hinglish/09 came back entirely in Telugu script (BENCH.md §16). Only Devanagari and Urdu are romanized. This needs a general script fallback.
5. **Rules gaps by design (ADR-0015):** Hindi number words, spoken punctuation, and self-corrections the recognizer garbled. For example, Nemotron's hinglish/11 has no date for "parson" to replace. All of these are left to polish.
6. **Per-segment polish doesn't fix a self-correction across a pause.** This is accepted in ADR-0016.
7. **The bench set has no fillers.** The filler pass is covered by golden cases and property tests, not measured on recordings.
8. **Unchanged from M1:** unsigned, CLI only, Windows only; one speaker, one mic; Groq, Sarvam and `parakeet-v3` not re-benchmarked.

## What M3 starts with

M3 is the Windows desktop app (Tauri v2 + Svelte, `apps/desktop`) on top of the engine:

- **Push-to-talk into any app**: a `WH_KEYBOARD_LL` hotkey, an overlay pill and a tray icon, insertion through the clipboard (HTML where the target takes it). The mic stays warm for about 2 minutes after a dictation (ADR-009). The overlay shows a waveform from `on_level` and partials from `on_partial`. When `on_polished` arrives, it replaces the instant text if the user hasn't typed since.
- **Target context**: detect the foreground app to choose Plain, Markdown or `code_mode`, and which apps opt in to polish.
- **`bolo-store`**: a settings file, history in SQLite, keys in Windows Credential Manager, and the user dictionary feeding polish's `dictionary`.
- **Installer and first run**: model download with the RAM-tier recommendation, and the 30 s idle model release.
- **Candidate follow-ups:** a script fallback for non-Devanagari output (gap 4), and checking `bolo run`'s Nemotron latency (gap 3).

M2 was signed off on 2026-11-10. The M3 plan is in `.scratch/m3-desktop/` (ADR-0017, ADR-0018).
