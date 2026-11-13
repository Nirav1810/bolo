# Code signing (Authenticode through SignPath)

The Windows installer is Authenticode-signed by [SignPath](https://signpath.io) with a certificate from [SignPath Foundation](https://signpath.org), which signs qualifying open-source projects for free ([ADR-0021](adr/0021-authenticode-signpath.md)). Until the project is approved and set up, signing stays off: the workflows skip it, releases ship unsigned exactly as before, and the README keeps the SmartScreen "More info → Run anyway" step.

## How it works

`release.yml` (tags) and `nightly.yml` (pushes to `main`) both build the NSIS installer with `npx tauri build`. With signing on, they then:

1. Upload the unsigned `*_x64-setup.exe` as a workflow artifact (`actions/upload-artifact` stores it as a zip).
2. Submit it with [`signpath/github-action-submit-signing-request`](https://github.com/SignPath/github-action-submit-signing-request) and wait for the signed copy. SignPath checks that the artifact came from this repository's workflow on a GitHub-hosted runner.
3. Check its signature with `Get-AuthenticodeSignature`. A release needs `Valid`; a nightly only needs a signature, because the test certificate isn't publicly trusted.
4. Replace the installer in `dist/` with the signed copy, then redo the **updater signature** (`npx tauri signer sign --app-version …`, which writes the `.sig`) and the **`.sha256`** (`scripts/release.py sha256`). Authenticode signing changes the file's bytes, so the `.sig` that `tauri build` wrote no longer matches it. `latest.json` is written afterwards from the new `.sig`, so the updater and the checksum both cover the file users download.
5. Carry on as before: smoke-test the installer, write the feed, publish.

| | Release (`v*` tag) | Nightly (`main`) |
|---|---|---|
| SignPath project | `bolo` | `bolo` |
| Signing policy | `release-signing` (Foundation certificate, manual approval) | `test-signing` (test certificate) |
| Artifact configuration | `default` | `default` |
| `productName` parameter | `Bolo` | `Bolo` (the same app, on the Nightly channel) |
| `version` parameter | the tag without `v` | the nightly version |

The artifact configuration is checked in at [`.signpath/artifact-configurations/default.xml`](../.signpath/artifact-configurations/default.xml). It signs the one `*_x64-setup.exe` in the zip and restricts its ProductName and ProductVersion to the parameters above, as SignPath Foundation requires. The release job waits up to an hour for approval; an unanswered request fails the run, and re-running the job submits a new one.

**Not signed:** `bolo-desktop.exe` inside the installer, and the CLI zip. SignPath can't deep-sign inside an NSIS installer, and SmartScreen judges the file that was downloaded, which is the installer. Signing the app exe would take a second signing request between `cargo build` and NSIS packaging (for example through Tauri's `bundle.windows.signCommand`). That's a follow-up if Windows or antivirus tools start flagging the installed exe.

## Switching it on

Signing runs when **both** of these are set in the GitHub repository (Settings → Secrets and variables → Actions):

| Kind | Name | Value |
|---|---|---|
| Secret | `SIGNPATH_API_TOKEN` | API token of a SignPath CI user that may submit to the `bolo` project |
| Variable | `SIGNPATH_ORGANIZATION_ID` | the SignPath organization ID (a GUID) |

Each job sets `SIGNPATH_SIGNING` from them, and every signing step has `if: env.SIGNPATH_SIGNING == 'true'`. Delete the secret to switch signing off again.

The updater keys (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) are unchanged and still required.

## Applying to SignPath Foundation

1. **Check the conditions** at [signpath.org/terms](https://signpath.org/terms). Bolo meets them as follows:
   - OSI-approved licence with no commercial dual licensing: MIT OR Apache-2.0.
   - Built in a verifiable way from this repository's source, by its own CI only. Both workflows build from a fresh checkout on GitHub-hosted runners.
   - A **Code signing policy** on the project's home page and release pages: the README section of that name gives the attribution ("Free code signing provided by SignPath.io, certificate by SignPath Foundation"), the team roles (committers and reviewers, approvers: the maintainer) and the privacy statement. Release notes should link to it too.
   - Multi-factor authentication for every team member on both GitHub and SignPath.
   - The download page describes what the software does (the README does).
2. **Apply** through the form linked from [signpath.org](https://signpath.org) with the repository URL and a short description. Wait for approval.
3. **Set up the organization** that SignPath creates:
   - Link the predefined **GitHub.com trusted build system** to the organization and to the project, and install the SignPath GitHub App on `Nirav1810/bolo`.
   - Create the project with slug `bolo`, linked to this repository.
   - Add the artifact configuration with slug `default`, pasting in `.signpath/artifact-configurations/default.xml`.
   - Check the signing policies: `release-signing` (Foundation certificate, approver = the maintainer) and `test-signing` (test certificate). SignPath Foundation usually creates both; keep these slugs or change them in the workflows.
   - Optionally add a GitHub build policy that requires GitHub-hosted runners and disallows re-runs of old builds.
   - Create a **CI user**, give it submitter rights on both policies, and generate its API token.
4. **Add the secret and the variable** above.

## Verifying

After the first signed nightly, download its installer and run in PowerShell:

```powershell
Get-AuthenticodeSignature .\Bolo-Nightly_*_x64-setup.exe | Format-List Status, StatusMessage, SignerCertificate
```

It shows a signature from the SignPath test certificate (`Status` isn't `Valid`, which is expected). Then cut a release, approve its signing request in SignPath, and check the published installer:

```powershell
Get-AuthenticodeSignature .\Bolo_X.Y.Z_x64-setup.exe | Format-List Status, SignerCertificate, TimeStamperCertificate
(Get-FileHash .\Bolo_X.Y.Z_x64-setup.exe -Algorithm SHA256).Hash   # matches the .sha256
```

`Status` must be `Valid`, with the signer issued to SignPath Foundation. Also check that the app's **Update** button installs that release (so the regenerated `.sig` matches), and that SmartScreen no longer blocks the download. A new certificate can still need a little download reputation before SmartScreen goes quiet. Once it does, drop the "More info → Run anyway" step from the README's install section.

If a signing request fails on the metadata restrictions, compare the installer's ProductName and ProductVersion (Properties → Details) with the `productName` and `version` parameters in the request.
