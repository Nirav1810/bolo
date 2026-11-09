# 08: M1 report and `v0.1.0`

**What to build:** First the M1 report:
- what shipped;
- accuracy and latency per engine, including Auto and Deepgram streaming;
- known gaps;
- what M2 starts with.

Then `v0.1.0` is cut through the release pipeline, and work stops until the user confirms M2 (milestone gate).

**Blocked by:** 04, 05, 06, 07

**Status:** ready-for-agent

- [ ] docs/BENCH.md and README show the final M1 numbers and explain how to download and run `bolo`.
- [ ] The release PR for `v0.1.0` is merged, and the tag pushed, only after the user approves the report.
- [ ] The `v0.1.0` GitHub Release is published as a prerelease, because the CLI preview is unsigned (ADR-0013).
- [ ] Work stops for M1 sign-off.
