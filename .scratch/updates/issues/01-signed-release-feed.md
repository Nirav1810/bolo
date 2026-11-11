# 01: Signed installers and a `latest.json` feed on releases

**What to build:** release builds that the updater can consume (ADR-0019).

**Blocked by:** none

**Status:** done

- [x] Generate a Tauri updater keypair (`tauri signer generate`). The public key goes in `tauri.conf.json` → `plugins.updater.pubkey`. The private key and password go in the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and the maintainer keeps an offline copy.
- [x] `bundle.createUpdaterArtifacts: true`, so `tauri build` writes `Bolo_X_x64-setup.exe.sig`. The release workflow passes the signing secrets to that step.
- [x] `release.py feed <tag> --url-base <base>` writes `dist/latest.json` with `version`, `notes` (the changelog section), `pub_date` and `platforms.windows-x86_64.{signature,url}`. It's unit-tested in `test_release.py`.
- [x] The release workflow uploads `latest.json` and the `.sig`.
- [x] `is_prerelease` is false for 0.x releases, so they publish as `--latest` and `/releases/latest/download/latest.json` resolves.

## Outcome

- Keypair made with `tauri signer generate`. The private key and password are at `%USERPROFILE%\.tauri\bolo-updater.key` (+ `.password`) on the maintainer's machine and in the repo secrets. The public key is in `tauri.conf.json` → `plugins.updater`, which also sets the Beta endpoint and `installMode: quiet`.
- `createUpdaterArtifacts: true`. A local signed build wrote `Bolo_0.3.0_x64-setup.exe.sig`. A build without the key set now fails at the signing step, so local installer builds need the two env vars.
- `release.py`: `package` copies the `.sig` when there is one; `feed vX --url-base URL [--notes TEXT]` writes `dist/latest.json` (`feed_json` is unit-tested); `is_prerelease` is now true only for old `-beta.N` versions.
- release.yml signs, writes the feed and uploads `.sig` + `latest.json`.
