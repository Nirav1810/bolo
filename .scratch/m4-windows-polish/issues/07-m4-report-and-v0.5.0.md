# 07: M4 report and v0.5.0

**What to build:** the milestone gate.

**Blocked by:** 01, 02, 03, 04, 05, 06

**Status:** ready-for-agent

- [ ] `docs/M4-REPORT.md`: what was built, numbers against the budgets (idle RAM and CPU, installer size, latency), known gaps, the manual checklist (with the M3 items carried over) and what M5 starts with.
- [ ] README, ARCHITECTURE.md and GLOSSARY.md are updated.
- [ ] `release.py prepare 0.5.0`, merge, tag. Verify the published installer and zip against their `.sha256`, and that the feed serves 0.5.0.
- [ ] **Human:** the acceptance run. On a fresh Windows VM or a clean user profile, go from the download to the first dictation in < 2 minutes.
- [ ] **Human:** M4 sign-off. Don't start M5 until then.
