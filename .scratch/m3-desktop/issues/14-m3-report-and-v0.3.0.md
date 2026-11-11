# 14: M3 report and v0.3.0

**What to build:** An M3 report like `docs/M2-REPORT.md`, and the `v0.3.0` prerelease (installer and CLI zip) through `scripts/release.py`. Then stop for the user's sign-off.

**Blocked by:** 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13

**Status:** ready-for-human (M3 sign-off)

- [x] `docs/M3-REPORT.md`:
  - what shipped;
  - key-up → text-in-Notepad latency p50/p95 over 20 dictations (engine release → text plus insertion);
  - idle RAM and CPU with the model unloaded and with it loaded;
  - installer size against 15 MB;
  - the polish swap rate in manual testing;
  - known gaps;
  - what M4 starts with.
- [x] A manual checklist for the human: Notepad, Word, Gmail in Chrome, Slack, VS Code, Windows Terminal, an elevated window, the polish swap, clean-profile install and uninstall.
- [x] ARCHITECTURE.md, README (install, Caps Lock behaviour, privacy) and `apps/desktop/README.md` are updated.
- [x] The `chore(release): v0.3.0` PR is merged, then the tag is pushed and the release published as a prerelease. The installer and zip are verified against their `.sha256` files.
- [ ] **Human:** M3 sign-off. Don't start M4 until then.

## Comments

**2026-11-11 (agent).**
- The M3 report and the README, ARCHITECTURE and desktop README updates merged as PR #42. The release PR merged as #43 (`2145e99`).
- `v0.3.0` was tagged and published as a prerelease. The workflow smoke-tested both the zip and the installer.
- The assets were downloaded through the API route, and `sha256sum -c` passed for `Bolo_0.3.0_x64-setup.exe` (10,245,824 bytes) and `bolo-v0.3.0-windows-x64.zip`. The unzipped `bolo --version` prints `bolo 0.3.0` with only Windows' own folders on PATH.
- What's left is the human's manual checklist in `docs/M3-REPORT.md`, then the M3 sign-off.
