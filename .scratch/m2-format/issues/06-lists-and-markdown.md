# 06: Lists and Markdown output

**What to build:** List detection and rendering, plus Markdown output (ADR-0015).

**Blocked by:** 03

**Status:** done

- [x] A list forms only from an explicit enumeration of at least 3 items: "first … second … third", "one … two … three", or "number one … number two …".
- [x] An optional lead-in ("I need three things:") becomes the intro line.
- [x] Prose joined with "and" never becomes a list, and two-item enumerations stay prose (golden cases).
- [x] Plain renders "1. …" for numbered speech and "- …" otherwise, one item per line. Markdown renders the same as real list syntax.
- [x] `FormattedOutput.plain` and `.markdown` are both filled from the one pipeline run.
- [x] The latency gate still passes, and formatted WER is re-measured.

**Agent notes (2026-11-10):**
- All three enumeration forms are numbered speech, so Layer 1 always renders "1. …". The "- …" form has no trigger yet, because ADR-0015 allows only these three, and the clip en/11 reference ("1. Update the dependencies.") uses numbers for ordinals. A bullet trigger, such as a spoken "bullet", would need an ADR change.
- Plain and Markdown differ only around a list. Markdown adds a blank line before and after it, so the list doesn't swallow the next line as a lazy continuation.
- Lists now run before numbers (ADR-0015's order is amended), to keep the pipeline idempotent.
- The idempotence property also turned up an older fillers bug: "you know" before ";" or "।". It is fixed, with a golden case.
- Formatted WER is re-measured in BENCH.md §15. Deepgram en goes 5.5% → 4.3%, and Nemotron is unchanged because it drops the spoken "First" on en/11.
