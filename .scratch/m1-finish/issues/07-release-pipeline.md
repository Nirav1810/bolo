# 07: Release pipeline

**What to build:** The release flow from ADR-0013:
- One command prepares a release PR.
- Pushing the tag publishes a GitHub Release with a Windows x64 `bolo` CLI zip and notes taken from the changelog.
- Beta tags publish prereleases.

It is proven end to end by publishing `v0.1.0-beta.1`.

**Blocked by:** 02

**Status:** ready-for-human (only the clean-profile run of the zip is left)

- [x] Release script:
  - takes `X.Y.Z` or `X.Y.Z-beta.N`;
  - refuses to run unless on a clean, up-to-date `main`;
  - creates a release branch, bumps the workspace version and regenerates `CHANGELOG.md` with git-cliff;
  - opens a `chore(release): vX.Y.Z` PR;
  - `--dry-run` prints the planned changes.
- [x] git-cliff config groups commits by Conventional Commit type and skips `chore(release)` commits.
- [x] CI runs the release script's dry run and checks that the version is consistent across manifests.
- [x] Tag workflow (`v*`):
  - builds `bolo` in release mode on Windows x64;
  - zips it with the sherpa-onnx shared libs, README and licence files;
  - publishes a GitHub Release with that version's changelog section;
  - marks `-beta.` tags as prereleases.
- [ ] `v0.1.0-beta.1` is published, and the downloaded zip runs `bolo --help` on a clean Windows machine or profile. Published as a prerelease (PR #9 + tag) and the tag workflow ran the unzipped `bolo --help` with only `System32` on `PATH`. A run on a clean Windows profile still needs a human.

## Comments

- 2026-11-09: Pipeline landed in the `ci(release)` PR. `scripts/release.py` (prepare / tag / check / notes / package / publish), `cliff.toml`, `.github/workflows/release.yml`. `bolo.exe` now links the MSVC CRT statically (`.cargo/config.toml`): before that it imported `VCRUNTIME140.dll`, which a clean Windows machine lacks. The tag workflow smoke-tests the unzipped CLI with only `System32` on PATH. 0.x releases are marked prerelease (unsigned preview, per ticket 08), and betas always are. Remaining: publish `v0.1.0-beta.1` and run the zip on a clean profile.
- 2026-11-09: `v0.1.0-beta.1` published (PR #9, prerelease, zip + `.sha256`). Fix PR #10 writes the checksum and notes files with LF endings. The agent side is done; the clean-profile check is the user's.
