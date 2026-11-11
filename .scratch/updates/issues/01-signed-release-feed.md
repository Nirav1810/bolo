# 01: Signed installers and a `latest.json` feed on releases

**What to build:** release builds that the updater can consume (ADR-0019).

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Generate a Tauri updater keypair (`tauri signer generate`). The public key goes in `tauri.conf.json` → `plugins.updater.pubkey`. The private key and password go in the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and the maintainer keeps an offline copy.
- [ ] `bundle.createUpdaterArtifacts: true`, so `tauri build` writes `Bolo_X_x64-setup.exe.sig`. The release workflow passes the signing secrets to that step.
- [ ] `release.py feed <tag> --url-base <base>` writes `dist/latest.json` with `version`, `notes` (the changelog section), `pub_date` and `platforms.windows-x86_64.{signature,url}`. It's unit-tested in `test_release.py`.
- [ ] The release workflow uploads `latest.json` and the `.sig`.
- [ ] `is_prerelease` is false for 0.x releases, so they publish as `--latest` and `/releases/latest/download/latest.json` resolves.
