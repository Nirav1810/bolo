# 14: M3 report and v0.3.0

**What to build:** An M3 report like `docs/M2-REPORT.md`, and the `v0.3.0` prerelease (installer and CLI zip) through `scripts/release.py`. Then stop for the user's sign-off.

**Blocked by:** 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13

**Status:** ready-for-agent

- [ ] `docs/M3-REPORT.md`:
  - what shipped;
  - key-up → text-in-Notepad latency p50/p95 over 20 dictations (engine release → text plus insertion);
  - idle RAM and CPU with the model unloaded and with it loaded;
  - installer size against 15 MB;
  - the polish swap rate in manual testing;
  - known gaps;
  - what M4 starts with.
- [ ] A manual checklist for the human: Notepad, Word, Gmail in Chrome, Slack, VS Code, Windows Terminal, an elevated window, the polish swap, clean-profile install and uninstall.
- [ ] ARCHITECTURE.md, README (install, Caps Lock behaviour, privacy) and `apps/desktop/README.md` are updated.
- [ ] The `chore(release): v0.3.0` PR is merged, then the tag is pushed and the release published as a prerelease. The installer and zip are verified against their `.sha256` files.
- [ ] **Human:** M3 sign-off. Don't start M4 until then.
