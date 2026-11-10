# 03: Rules pipeline, test harness, punctuation fix-ups and commands

**What to build:** The Layer 1 rules pipeline in `bolo-format` (ADR-0015), wired into the worker after transliteration and the loanword lexicon and timed as `rules_ms`. This ticket builds the test harness, the latency gate and the first passes: punctuation fix-ups, the "new line" and "new paragraph" commands, and Plain/`code_mode` rendering. Later passes plug in at their place in the order.

**Blocked by:** none

**Status:** done

- [x] One public entry point takes the text, the language (English/Hinglish) and a format context (Plain/Markdown, `code_mode`) that `bolo-format` defines itself. `bolo-core` maps `TargetContext` onto it.
- [x] The worker runs the pipeline on every final, and `rules_ms` measures it.
- [x] The golden harness runs `crates/bolo-format/tests/golden/*.toml` (`input`, `expected`, `note`, optional `language` and `format`) in one test, and each failing case is reported by its `note`.
- [x] Property tests: the pipeline is idempotent, its output is non-empty for non-empty input, and these passes drop no words.
- [x] Punctuation fix-ups: sentence-start capitals, a final period, a capital "I", spacing around punctuation, and no doubled punctuation. The pass never adds commas.
- [x] Commands: "new line" and "new paragraph", with Hinglish forms ("nayi line", "naya paragraph"). "comma" and "full stop" are left as words.
- [x] `code_mode`: no sentence capitals and no final period.
- [x] Latency gate: the full pipeline is under 1 ms p95 over the 40 clips' transcripts, enforced by a test or a CI-light bench.
- [x] Formatted WER is re-measured, and BENCH.md is updated. `bolo run` scores the raw transcript (romanized), so it must run the pipeline on it before scoring and before `--polish`; `bolo replay` gets the rules through the engine.

## Comments

**2026-11-10 (agent):** Done.
- Entry point: `bolo_format::rules::apply(text, Language, FormatContext)`.
- Golden cases are in `crates/bolo-format/tests/golden/`, one file per pass plus `end_to_end.toml`. The property tests are in `tests/properties.rs` and the latency gate in `tests/latency.rs` (p95 0.027 ms, debug build).
- Choices made where the ticket was open:
  - **Commands.** A command is not taken after a determiner ("a new line of products") or before "of". A command at the very start or end keeps its break, so the next dictation starts on a new line. A comma or semicolon before the break is dropped.
  - **Final period.** None is added when the text ends on a break. Devanagari text ends with "।".
  - **`code_mode`.** Also skips the capital "I" (no automatic capitals, user story 14). It removes the engine's final period and lowercases a Titlecase first word ("Get user by ID." → "get user by ID").
  - **Sentence capitals.** These skip URLs, emails and mixed-case words ("example.com", "iPhone").
  - **Auto language.** When the engine's own multilingual mode decided (Deepgram `multi`), the pipeline language comes from `rules::Language::of_engine_output`: Hinglish only when Devanagari output shows Hindi function words, otherwise English.
  - **HTML targets.** These get Plain until M3.
- Formatted WER was re-measured with no change (BENCH.md §12).
