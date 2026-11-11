# 02: Nightly builds from `main`

**What to build:** a workflow that publishes a signed nightly installer for every push to `main` (ADR-0019).

**Blocked by:** 01

**Status:** done

- [x] `release.py nightly-version` prints the nightly version: the next minor after the newest `vX.Y.Z` tag, plus `-nightly.<YYYYMMDD>.<run>`, using the UTC date and `GITHUB_RUN_NUMBER`. `release.py set-version <v>` writes it into the manifests, and nothing is committed. Both are unit-tested.
- [x] Check that `tauri build --bundles nsis` accepts the prerelease version. If it doesn't, pick a format it does accept and amend the ADR.
- [x] `.github/workflows/nightly.yml` runs on `push: main` and `workflow_dispatch`, with a concurrency group that cancels older runs. It builds the signed installer, smoke-installs it as release.yml does, writes `latest.json` with URLs under `releases/download/nightly/`, moves the `nightly` tag to the built commit, and replaces the `nightly` prerelease's assets.
- [x] The nightly release notes name the commit and link the changes since the last release.
- [x] A docs-only push skips the build (paths filter).

## Outcome

- `release.py nightly-version --run N` and `set-version`, both unit-tested. `parse_tag` accepts nightly names, and `release_tags` ignores them.
- `tauri build --bundles nsis` accepts `0.4.0-nightly.20261111.1` (checked locally): `Bolo_0.4.0-nightly.20261111.1_x64-setup.exe` + `.sig`. No change to the ADR needed.
- `.github/workflows/nightly.yml`:
  - runs on push to `main` (ignoring docs, `.scratch` and `*.md`) and on dispatch, with older runs cancelled;
  - builds signed, smoke-installs and writes `latest.json` with URLs under `releases/download/nightly/`;
  - moves the `nightly` tag, then uploads the new assets (feed last) and deletes the old ones;
  - the notes name the commit and link the compare view since the last release.
- First real run: on the merge of this PR.
