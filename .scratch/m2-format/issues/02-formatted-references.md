# 02: Formatted references and formatted WER

**What to build:** Each of the 40 bench clips gets a **formatted reference**: the text the user would actually send, written in the M2 style (ADR-0015 number style, fillers removed, corrections applied, lists, capitals). `bolo run` and `bolo replay` report formatted WER next to verbatim WER. A flag adds a Layer 1 + polish column.

**Blocked by:** none

**Status:** ready-for-human

- [x] Formatted references are drafted for all 40 clips and stored next to the verbatim references, in the same format.
- [x] `bolo run` and `bolo replay` print and save formatted WER next to verbatim WER. Old result files still load.
- [x] A flag (e.g. `--polish`) adds a Layer 1 + polish column. Without it, no API calls are made.
- [x] The baseline formatted WER (before rules) is recorded in BENCH.md.
- [ ] **Human:** the user reviews and corrects the drafted references. The formatted references are final once the user signs them off.

**Agent notes (2026-11-10):** drafts are `testdata/clips/*/*.formatted.txt`; 7 of 40 differ from verbatim (listed in BENCH.md §11). Two need the user's call: hinglish/11 ("Kal shaam ko chalte hain, nahi wait, parson chalte hain" → "Parson chalte hain." drops "shaam ko"), and hinglish/15 ("pandrah" → "15", which Layer 1 will never produce). en/11 is a numbered list in plain text.
