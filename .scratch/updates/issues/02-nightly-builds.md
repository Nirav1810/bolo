# 02: Nightly builds from `main`

**What to build:** a workflow that publishes a signed nightly installer for every push to `main` (ADR-0019).

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] `release.py nightly-version` prints the nightly version: the next minor after the newest `vX.Y.Z` tag, plus `-nightly.<YYYYMMDD>.<run>`, using the UTC date and `GITHUB_RUN_NUMBER`. `release.py set-version <v>` writes it into the manifests, and nothing is committed. Both are unit-tested.
- [ ] Check that `tauri build --bundles nsis` accepts the prerelease version. If it doesn't, pick a format it does accept and amend the ADR.
- [ ] `.github/workflows/nightly.yml` runs on `push: main` and `workflow_dispatch`, with a concurrency group that cancels older runs. It builds the signed installer, smoke-installs it as release.yml does, writes `latest.json` with URLs under `releases/download/nightly/`, moves the `nightly` tag to the built commit, and replaces the `nightly` prerelease's assets.
- [ ] The nightly release notes name the commit and link the changes since the last release.
- [ ] A docs-only push skips the build (paths filter).
