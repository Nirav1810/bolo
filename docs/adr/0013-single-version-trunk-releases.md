# One product version, squash-merged trunk, stable + beta channels

Numbering continues from `docs/DECISIONS.md` (ADR-001…012), which is the predecessor log.

Bolo ships a Windows app, an Android app and Rust crates from one repo. We use **one product version** for all of them (`vX.Y.Z` tags; Android `versionCode` is derived from it) rather than per-platform tags, so a version means the same feature set everywhere and the changelog is a single file. `main` is protected: changes land only through **squash-merged PRs** whose title is a Conventional Commit (linted in CI), so every commit on `main` passed CI and git-cliff can generate release notes straight from history. Releases are cut from tags on `main` in two **release channels**: stable (`v0.4.0`) and beta (`v0.4.0-beta.1`), mapping to separate Tauri updater feeds and to Play production / closed testing. No nightly channel; CI artifacts on `main` cover "latest".

The release pipeline starts at the end of M1 (`v0.1.0`: changelog + GitHub Release) and gains one platform per milestone (Windows installer in M3, Android in M5) instead of being built all at once in M4, so the release mechanics are exercised many times before the first real user installs.

A release is cut by `scripts/release.py X.Y.Z`, which opens a `chore(release): vX.Y.Z` PR that bumps the version in every manifest (Cargo workspace, later `tauri.conf.json` and the Android Gradle file) and regenerates `CHANGELOG.md` with git-cliff; after it merges, pushing the tag runs the release workflow. The crates are `publish = false`: they are internal to the apps, and publishing would freeze their APIs and add a step to every release. `v0.1.0` ships source, changelog and an unsigned Windows x64 zip of the CLI (renamed `bolo-bench` → `bolo`) with its sherpa-onnx DLLs, marked prerelease.

## Considered Options

- **Per-platform versions** (`desktop-v…`, `android-v…`): rejected while platforms ship together; revisit only if their cadences diverge.
- **Merge commits / rebase merges**: rejected; they put work-in-progress commits on `main` and into the changelog.
- **release-please bot**: rejected for now; needs custom updaters for the Tauri and Gradle version fields, and a script keeps the release moment in our hands.
