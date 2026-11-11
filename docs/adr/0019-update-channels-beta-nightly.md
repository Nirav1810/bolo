# Self-update, and Bolo Nightly as a separate app next to Bolo

Amends ADR-0013's channel model. The one-product-version rule, the squash-merged trunk and the release script stay as they are.

There are two **release channels**, and each ships as its own Windows app (like a browser's stable and canary builds):

- **Bolo**: every `vX.Y.Z` release cut by `scripts/release.py`. While the product version is 0.x the app shows a **Beta** tag. From `v1.0.0` on the tag is dropped. We stop cutting `-beta.N` tags: one tag per release, and the badge carries the "not finished" meaning. `release.py` still accepts the suffix, so the old tags keep sorting right.
- **Bolo Nightly**: CI builds a signed installer on every push to `main`.
  - Its version is the next minor after the newest release tag plus `-nightly.<YYYYMMDD>.<run>`, for example `0.4.0-nightly.20261111.7`. That sorts below `0.4.0` in semver. It is written into the manifests for that build only and is never committed.
  - It builds with `tauri.nightly.conf.json` laid over the app's config, which gives it its own product name, identifier (`dev.bolo.desktop.nightly`), install folder (`%LOCALAPPDATA%\Bolo Nightly`), Start menu entry, autostart entry and **violet icon**.
  - Each build replaces the assets of the one rolling `nightly` GitHub prerelease, whose tag moves to the built commit. The installer is published as `Bolo-Nightly_<version>_x64-setup.exe`, because GitHub turns spaces into dots.

The two apps install side by side. A Nightly is never taken for Bolo:
- different name and icon in the taskbar, Start menu, tray and window titles;
- "Bolo Nightly 0.5.0-nightly…" at the top of the tray menu;
- a violet Nightly block at the foot of the settings sidebar on every page.

They share the settings (`%APPDATA%\Bolo`), models, history and the API keys in Credential Manager. Each writes its own log. Only one holds the push-to-talk key: when either installs its keyboard hook (at start, or on Resume), it sets the other's named event `Local\dev.bolo.desktop[.nightly].step-aside`. The other then pauses and says so in the pill. Bolo Nightly's uninstaller leaves the shared folders alone.

Each app updates only from its own feed, a static `latest.json` (Tauri's updater format) attached as a release asset:
- Bolo: `https://github.com/Nirav1810/bolo/releases/latest/download/latest.json`. For `/releases/latest/` to resolve, releases are no longer marked prerelease on GitHub. The Beta tag in the app replaces that flag.
- Bolo Nightly: `https://github.com/Nirav1810/bolo/releases/download/nightly/latest.json`.

The app checks on launch and every 6 hours, and never installs without a click. When a newer build is out, Settings → About and the tray offer **Update**. Clicking it downloads the installer, checks its minisign signature against the public key compiled into the app, runs it quietly (per-user NSIS needs no elevation), and the installer restarts the app. About links to the other app's download page instead of a channel switch.

The installers are signed with a Tauri updater key (minisign). The private key and its password live in the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, with an offline copy kept by the maintainer. Losing the key strands every install on its current version, because a new key needs a manual reinstall. Authenticode signing is a separate decision (M4).

## Considered Options

- **One app with a channel picker** (first version of this ADR): rejected. The user wanted to see at a glance which build runs. One install that changes its name and icon on update leaves Windows shortcuts and the uninstall entry behind the rename.
- **Keep stable + beta (`-beta.N`) and add nightly**: rejected. Three channels for a pre-1.0 app with one maintainer is more to cut and explain. "Beta until 1.0" carries the same meaning.
- **Hand-cut alpha tags instead of nightly**: rejected. Every merge already passes CI, and an automatic build is what the UI work needs for fast try-it-yourself loops.
- **Separate settings per app**: rejected. Trying a nightly shouldn't mean setting Bolo up again. The settings file is versioned and keeps unknown keys, so the two apps can share it across versions.
- **Fully automatic install**: rejected for now. Dictation is something you're in the middle of. Restarting under the user is worse than one click.
- **Auto-update on the CLI zip**: out of scope. The CLI is a developer tool.
