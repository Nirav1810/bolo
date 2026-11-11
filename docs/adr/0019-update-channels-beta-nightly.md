# Self-update with two channels: Beta (releases) and Nightly (main)

Amends ADR-0013's channel model. The one-product-version rule, the squash-merged trunk and the release script stay as they are.

The Windows app updates itself with `tauri-plugin-updater`. It reads one of two **release channels**, and the user picks which one in Settings → About:

- **Beta**: every `vX.Y.Z` release cut by `scripts/release.py`. While the product version is 0.x, the app shows these builds with a **Beta** badge. From `v1.0.0` on, the badge is dropped and the channel is plain releases. We stop cutting `-beta.N` tags: one tag per release, and the badge carries the "not finished" meaning. `release.py` still accepts the suffix, so the old tags keep sorting right.
- **Nightly**: CI builds a signed installer on every push to `main`. Its version is the next minor after the newest release tag plus `-nightly.<YYYYMMDD>.<run>`, for example `0.4.0-nightly.20261111.7`. That sorts below `0.4.0` in semver, so the release that ships the work overtakes the nightlies. The version is written into the manifests for that build only and is never committed. Each build replaces the assets of the one rolling `nightly` GitHub prerelease, whose tag moves to the built commit. The app shows a **Nightly** badge.

Feeds are static `latest.json` files (Tauri's updater format) attached as release assets:
- Beta: `https://github.com/Nirav1810/bolo/releases/latest/download/latest.json`. For `/releases/latest/` to resolve, releases are no longer marked prerelease on GitHub. The Beta badge in the app replaces that flag.
- Nightly: `https://github.com/Nirav1810/bolo/releases/download/nightly/latest.json`.

The app checks on launch and every 6 hours, and never installs without a click. When a newer build is on the user's channel, Settings → About and the tray offer **Update**. Clicking it downloads the installer, checks its minisign signature against the public key compiled into the app, runs it quietly (per-user NSIS needs no elevation), and restarts Bolo. Updates only go forward. A user who switches from Nightly back to Beta stays on the nightly until a release newer than it ships.

The installers are signed with a Tauri updater key (minisign). The private key and its password live in the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, with an offline copy kept by the maintainer. Losing the key strands every install on its current version, because a new key needs a manual reinstall. Authenticode signing is a separate decision (M4).

## Considered Options

- **Keep stable + beta (`-beta.N`) and add nightly**: rejected. Three channels for a pre-1.0 app with one maintainer is more to cut and explain. "Beta until 1.0" carries the same meaning.
- **Hand-cut alpha tags instead of nightly**: rejected. Every merge already passes CI, and an automatic build is what the UI work needs for fast try-it-yourself loops.
- **One feed file on a fixed `channels` release**: rejected. It would need write access to update from both workflows and adds a release that isn't one. `/releases/latest/` plus the rolling `nightly` tag give each channel its own URL for free.
- **Fully automatic install**: rejected for now. Dictation is something you're in the middle of. Restarting under the user is worse than one click.
- **Auto-update on the CLI zip**: out of scope. The CLI is a developer tool.
