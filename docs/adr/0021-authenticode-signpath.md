# Authenticode signing through SignPath Foundation

The installer is unsigned, so Windows SmartScreen warns on every download until the file builds reputation. The original spec names SignPath Foundation (free for qualifying open-source projects) or Azure Artifact Signing. The maintainer chose **SignPath Foundation**.

CI builds the unsigned installer, uploads it as a workflow artifact and submits it to SignPath with `signpath/github-action-submit-signing-request`. It waits for the signed file, then computes the Tauri updater signature and the `.sha256` over the **signed** file, so updates and checksums match what users download. The certificate belongs to SignPath Foundation, and the project has to follow its code-signing policy: an OSI licence, a signing policy page in the repo, and releases built only from this repo's CI.

The workflows sign only when the `SIGNPATH_API_TOKEN` secret is present. Until the application is approved, releases ship unsigned and the README keeps the "More info → Run anyway" step. Nightlies are signed with the same project once it exists, and they go through a separate SignPath signing policy so that test builds never use the release policy.

## Considered Options

- **Azure Artifact Signing:** about $10 a month, and individual eligibility depends on the country. It stays the fallback if SignPath declines.
- **A self-bought OV certificate:** costs hundreds of dollars a year and needs a hardware token, which doesn't fit CI on hosted runners.
- **Stay unsigned:** this was the status quo, and it fails the milestone's "fresh install in < 2 minutes" spirit for non-technical users.
