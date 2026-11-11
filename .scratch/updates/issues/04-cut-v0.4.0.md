# 04: Cut v0.4.0 and check updates end to end

**What to build:** the first release that can update itself.

**Blocked by:** 01, 02, 03

**Status:** ready-for-agent

- [ ] `release.py 0.4.0` opens the release PR. Merge it, tag it, and the workflow publishes the release as Latest with `latest.json`.
- [ ] The release feed serves 0.4.0, and the Nightly feed serves a `0.5.0-nightly.*` Bolo Nightly build.
- [ ] Install v0.4.0 by hand (v0.3.0 has no updater) and Bolo Nightly from the `nightly` release. They sit side by side with separate Start menu entries and icons.
- [ ] Push something to `main`. The installed Bolo Nightly offers the next nightly, updates to it and restarts. Bolo isn't offered anything.
- [ ] Starting either app pauses the other (pill notice). Resuming one from the tray pauses the other.
- [ ] Record the results here and in the README's install section.
