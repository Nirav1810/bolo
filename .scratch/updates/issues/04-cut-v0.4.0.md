# 04: Cut v0.4.0 and check updates end to end

**What to build:** the first release that can update itself.

**Blocked by:** 01, 02, 03

**Status:** ready-for-agent

- [ ] `release.py 0.4.0` opens the release PR. Merge it, tag it, and the workflow publishes the release as Latest with `latest.json`.
- [ ] The Beta feed URL serves 0.4.0, and the Nightly feed serves a `0.5.0-nightly.*` build.
- [ ] Install v0.4.0 by hand (v0.3.0 has no updater). Switch to Nightly; About offers the nightly; Update installs it and Bolo restarts as the nightly with the Nightly badge.
- [ ] Push something to `main`. The installed nightly offers the next nightly and updates to it.
- [ ] Switch back to Beta: no downgrade is offered.
- [ ] Record the results here and in the README's install section.
