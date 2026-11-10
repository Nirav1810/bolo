# 03: Rules pipeline, test harness, punctuation fix-ups and commands

**What to build:** The Layer 1 rules pipeline in `bolo-format` (ADR-0015), wired into the worker after transliteration and the loanword lexicon and timed as `rules_ms`. This ticket builds the test harness, the latency gate and the first passes: punctuation fix-ups, the "new line" and "new paragraph" commands, and Plain/`code_mode` rendering. Later passes plug in at their place in the order.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] One public entry point takes the text, the language (English/Hinglish) and a format context (Plain/Markdown, `code_mode`) that `bolo-format` defines itself. `bolo-core` maps `TargetContext` onto it.
- [ ] The worker runs the pipeline on every final, and `rules_ms` measures it.
- [ ] The golden harness runs `crates/bolo-format/tests/golden/*.toml` (`input`, `expected`, `note`, optional `language` and `format`) in one test, and each failing case is reported by its `note`.
- [ ] Property tests: the pipeline is idempotent, its output is non-empty for non-empty input, and these passes drop no words.
- [ ] Punctuation fix-ups: sentence-start capitals, a final period, a capital "I", spacing around punctuation, and no doubled punctuation. The pass never adds commas.
- [ ] Commands: "new line" and "new paragraph", with Hinglish forms ("nayi line", "naya paragraph"). "comma" and "full stop" are left as words.
- [ ] `code_mode`: no sentence capitals and no final period.
- [ ] Latency gate: the full pipeline is under 1 ms p95 over the 40 clips' transcripts, enforced by a test or a CI-light bench.
- [ ] Formatted WER is re-measured, and BENCH.md is updated.
