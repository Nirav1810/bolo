# 06: Authenticode signing through SignPath

**What to build:** ADR-0021. Sign the installer and `bolo-desktop.exe` in CI, and leave it off until the SignPath Foundation project is approved.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `release.yml` and `nightly.yml`: when the `SIGNPATH_API_TOKEN` secret and the `SIGNPATH_ORGANIZATION_ID` variable are set, upload the unsigned build as an artifact, submit it with `signpath/github-action-submit-signing-request`, and use the signed files that come back. The updater signature (`.sig`) and the `.sha256` are computed **after** Authenticode signing. Without the secret, the workflows run as they do today.
- [ ] `.signpath/` holds the artifact configuration: the installer, plus the app exe inside it if SignPath's NSIS support allows. A `docs/SIGNING.md` covers how to apply (OSS conditions, the code-signing policy page, and the secret and variable names) and how to switch signing on.
- [ ] The README's install section says signing is coming, and keeps the SmartScreen step until it's on.
- [ ] **Human:** apply to SignPath Foundation, create the project, and add the token and organization id. Then cut a release and check that the installer's signature is valid (`Get-AuthenticodeSignature`) and that SmartScreen stays quiet.
