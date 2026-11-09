# 07: Release pipeline

**What to build:** The release flow from ADR-0013:
- One command prepares a release PR.
- Pushing the tag publishes a GitHub Release with a Windows x64 `bolo` CLI zip and notes taken from the changelog.
- Beta tags publish prereleases.

It is proven end to end by publishing `v0.1.0-beta.1`.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] Release script:
  - takes `X.Y.Z` or `X.Y.Z-beta.N`;
  - refuses to run unless on a clean, up-to-date `main`;
  - creates a release branch, bumps the workspace version and regenerates `CHANGELOG.md` with git-cliff;
  - opens a `chore(release): vX.Y.Z` PR;
  - `--dry-run` prints the planned changes.
- [ ] git-cliff config groups commits by Conventional Commit type and skips `chore(release)` commits.
- [ ] CI runs the release script's dry run and checks that the version is consistent across manifests.
- [ ] Tag workflow (`v*`):
  - builds `bolo` in release mode on Windows x64;
  - zips it with the sherpa-onnx shared libs, README and licence files;
  - publishes a GitHub Release with that version's changelog section;
  - marks `-beta.` tags as prereleases.
- [ ] `v0.1.0-beta.1` is published, and the downloaded zip runs `bolo --help` on a clean Windows machine or profile.
