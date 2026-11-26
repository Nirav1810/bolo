# 10: M5 report and v0.6.0

**What to build:** the emulator acceptance run, docs/M5-REPORT.md and the `v0.6.0` release. This is the milestone gate: stop for the user's sign-off.

**Blocked by:** 01, 02, 03, 04, 05, 06, 07, 08, 09

**Status:** ready-for-agent

- [ ] Acceptance run on the emulator, from a clean AVD: install the CI debug APK, do onboarding, then dictate in Chrome, a mail app, a messaging app and Keep/Notes (English and Hinglish), and check polish replace, cancel, a password field and a process kill mid-idle. Record the results with screenshots.
- [ ] docs/M5-REPORT.md: what shipped, the budgets table (emulator numbers), BENCH sections, deviations, the open Human checks (real phones; the M3/M4 checklists still owed) and "What M6 starts with" (signing, Play Store, F-Droid, release APKs).
- [ ] README: an Android section (sideload the debug APK from CI, enable the keyboard).
- [ ] Release `v0.6.0` with `release.py` as usual. Desktop gets everything since v0.5.1 (the channel picker and the new UI). The Android APK is not a release asset until M6.
- [ ] Stop and wait for the user's sign-off.
