# 10: M2 report and v0.2.0

**What to build:** An M2 report like `docs/M1-REPORT.md`, and the `v0.2.0` prerelease through `scripts/release.py`. Then stop for the user's sign-off.

**Blocked by:** 01, 02, 03, 04, 05, 06, 07, 08, 09

**Status:** ready-for-human

- [x] `docs/M2-REPORT.md`:
  - what shipped;
  - formatted and verbatim WER for Layer 1 and Layer 1 + polish;
  - the release → text latency and the rules latency;
  - per-segment polish numbers;
  - known gaps;
  - what M3 starts with.
- [x] ARCHITECTURE.md and README are updated for the rules pipeline and `on_polished`.
- [x] The `chore(release): v0.2.0` PR is merged, then the tag is pushed and the release published as a prerelease. The zip is verified.
  - Report PR #24 (a1ade34), release PR #25 (8623c73). `v0.2.0` was published 2026-11-10 as a prerelease. The zip's SHA-256 matches, and the unzipped `bolo --version` prints `bolo 0.2.0` with only System32 on `PATH`.
- [ ] **Human:** M2 sign-off. Don't start M3 until then.
