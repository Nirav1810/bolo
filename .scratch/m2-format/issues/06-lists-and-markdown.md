# 06: Lists and Markdown output

**What to build:** List detection and rendering, plus Markdown output (ADR-0015).

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] A list forms only from an explicit enumeration of at least 3 items: "first … second … third", "one … two … three", or "number one … number two …".
- [ ] An optional lead-in ("I need three things:") becomes the intro line.
- [ ] Prose joined with "and" never becomes a list, and two-item enumerations stay prose (golden cases).
- [ ] Plain renders "1. …" for numbered speech and "- …" otherwise, one item per line. Markdown renders the same as real list syntax.
- [ ] `FormattedOutput.plain` and `.markdown` are both filled from the one pipeline run.
- [ ] The latency gate still passes, and formatted WER is re-measured.
