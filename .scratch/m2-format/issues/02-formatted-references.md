# 02: Formatted references and formatted WER

**What to build:** Each of the 40 bench clips gets a **formatted reference**: the text the user would actually send, written in the M2 style (ADR-0015 number style, fillers removed, corrections applied, lists, capitals). `bolo run` and `bolo replay` report formatted WER next to verbatim WER. A flag adds a Layer 1 + polish column.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Formatted references are drafted for all 40 clips and stored next to the verbatim references, in the same format.
- [ ] `bolo run` and `bolo replay` print and save formatted WER next to verbatim WER. Old result files still load.
- [ ] A flag (e.g. `--polish`) adds a Layer 1 + polish column. Without it, no API calls are made.
- [ ] The baseline formatted WER (before rules) is recorded in BENCH.md.
- [ ] **Human:** the user reviews and corrects the drafted references. The formatted references are final once the user signs them off.
