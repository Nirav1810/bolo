# 04: Cut v0.4.0 and check updates end to end

**What to build:** the first release that can update itself.

**Blocked by:** 01, 02, 03

**Status:** ready-for-human (the install and live update checks below)

- [x] `release.py 0.4.0` opens the release PR. Merge it, tag it, and the workflow publishes the release as Latest with `latest.json`.
- [x] The release feed serves 0.4.0, and the Nightly feed serves a `0.5.0-nightly.*` Bolo Nightly build.
- [ ] Install v0.4.0 by hand (v0.3.0 has no updater) and Bolo Nightly from the `nightly` release. They sit side by side with separate Start menu entries and icons.
- [ ] Push something to `main`. The installed Bolo Nightly offers the next nightly, updates to it and restarts. Bolo isn't offered anything.
- [ ] Starting either app pauses the other (pill notice). Resuming one from the tray pauses the other.
- [ ] Record the results here and in the README's install section.

## Results

- 2026-11-12: release PR #57 merged as 468de54, tag `v0.4.0` pushed. The release workflow published **Bolo v0.4.0** as Latest with `Bolo_0.4.0_x64-setup.exe` (+ `.sig`, `.sha256`), the CLI zip and `latest.json`. Both `.sha256` files check out.
- `releases/latest/download/latest.json` serves `0.4.0` (windows-x86_64, signed). `releases/download/nightly/latest.json` serves `0.5.0-nightly.20261112.15`.
- Merging this record pushes to `main`, so the next nightly (`0.5.0-nightly.20261112.16` or later) is the one an installed Bolo Nightly `.15` should be offered.
- Owed by the human: the hand install of both apps, the Nightly self-update, the pause/resume check, then the README install section.
