# 07: M4 report and v0.5.0

**What to build:** the milestone gate.

**Blocked by:** 01, 02, 03, 04, 05, 06

**Status:** ready-for-human (the acceptance run and M4 sign-off)

- [x] `docs/M4-REPORT.md`: what was built, numbers against the budgets (idle RAM and CPU, installer size, latency), known gaps, the manual checklist (with the M3 items carried over) and what M5 starts with.
- [x] README, ARCHITECTURE.md and GLOSSARY.md are updated.
- [x] `release.py prepare 0.5.0`, merge, tag. Verify the published installer and zip against their `.sha256`, and that the feed serves 0.5.0.
- [ ] **Human:** the acceptance run. On a fresh Windows VM or a clean user profile, go from the download to the first dictation in < 2 minutes.
- [ ] **Human:** M4 sign-off. Don't start M5 until then.

## Outcome

- The M4 report is PR #66 (9006c87).
- The release PR #67 failed `rust (windows-latest)` on a property-test flake. `commands_drop_only_their_own_words` generated text ending in "new", so splicing in "new paragraph" made a stutter. That was a test problem, fixed in #68 (6ca9b10). #67 was closed and the release was prepared again as #69 (eb4c3ae).
- `v0.5.0` was published 2026-11-12 as Latest.
  - `Bolo_0.5.0_x64-setup.exe` (10.2 MiB) and the CLI zip check out against their `.sha256` (downloaded through the API asset route).
  - `bolo --version` prints `bolo 0.5.0` with only System32 on PATH.
  - `releases/latest/download/latest.json` serves 0.5.0, signed.
  - The Nightly feed has moved on to `0.6.0-nightly.*`.
- The installer is still unsigned, because the SignPath application is owed (ticket 06).
