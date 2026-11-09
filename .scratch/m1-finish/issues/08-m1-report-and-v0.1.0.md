# 08: M1 report and `v0.1.0`

**What to build:** First the M1 report:
- what shipped;
- accuracy and latency per engine, including Auto and Deepgram streaming;
- known gaps;
- what M2 starts with.

Then `v0.1.0` is cut through the release pipeline, and work stops until the user confirms M2 (milestone gate).

**Blocked by:** 04, 05, 06, 07

**Status:** ready-for-human (M1 sign-off)

- [x] docs/BENCH.md and README show the final M1 numbers and explain how to download and run `bolo`.
- [x] The release PR for `v0.1.0` is merged, and the tag pushed, only after the user approves the report.
- [x] The `v0.1.0` GitHub Release is published as a prerelease, because the CLI preview is unsigned (ADR-0013).
- [ ] Work stops for M1 sign-off.

## Comments

- 2026-11-09: M1 report written in `docs/M1-REPORT.md`; BENCH.md §9 and README ("Try it") carry the final numbers and download steps. Waiting for the user to approve the report before `python -I scripts/release.py 0.1.0`.
- 2026-11-09: Report approved by the user. Release PR #12 merged (c8276d0), `v0.1.0` tagged, and the release workflow published https://github.com/Nirav1810/bolo/releases/tag/v0.1.0 as a prerelease (zip + `.sha256`). The downloaded zip passed its checksum and ran `bolo --version` (0.1.0) and `--help` with only `System32` on `PATH`. Work stops for M1 sign-off.
