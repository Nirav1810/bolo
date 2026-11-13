# Self-update, and Beta and Nightly as channels of one app

Amends ADR-0013's channel model. The one-product-version rule, the squash-merged trunk and the release script stay as they are.

*Amended 2026-11-13 (one app, two channels):* Bolo Nightly is no longer a separate app. The user asked for T3 Code's model: one app with an **update channel** (Beta or Nightly) in Settings → About.
- A nightly build is the same app as a release: same product name, identifier (`dev.bolo.desktop`) and install folder. `tauri.nightly.conf.json` now only swaps in the violet icons.
- `[updates] channel` (`beta` or `nightly`) holds the pick; unpicked, Bolo follows the channel of the build that runs.
- Picking the other channel checks that channel's feed and installs its newest build straight away, even when it is older (Nightly → Beta steps down in semver). Bolo restarts as that build, so the name ("Bolo" or "Bolo Nightly" in titles and the tray), icon, pill colour and features all change together. The branding always follows the build that runs, never the pick alone: a Beta exe with a violet icon would pass for a nightly.
- On the build's own channel, updates still only go forward.
- The step-aside events and the per-app on/off switches (`[variants]`) are gone with the second app. `[variants]` stays in the file for older builds; the merged app resets it to on, so switching to a Beta from before the merge doesn't start paused.
- On launch, the merged app removes the old separate Bolo Nightly install, if there is one: it asks it to step aside, then runs its uninstaller quietly (`/S`). Tauri's uninstaller closes only the exe at its own path (Restart Manager), removes its shortcuts and start-with-Windows entry, and leaves the shared settings, models and history alone.
- The old app's last update (to a merged nightly) installs over Bolo, since the installer goes by product name. The merged app also reads the old app's `updating-from-nightly.txt`, so that launch still opens About.
- A Beta from before the merge (v0.5.1 and older) has no channel picker. Switching to Beta lands on it until the next release, which has the picker.

The text below describes the two-app design this replaces, kept for the record.

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

Each app also has an on/off switch, `[variants] release` and `nightly` in the shared settings, shown on the About page of both. An app that is off stays in the tray, paused: no hook, no pill, no model, and it stays off after a restart or an update. Its tray's Resume turns it back on. Changing the switches sets the other app's `Local\dev.bolo.desktop[.nightly].switches-changed` event, and the other app reads them from the file again.

Each app updates only from its own feed, a static `latest.json` (Tauri's updater format) attached as a release asset:
- Bolo: `https://github.com/Nirav1810/bolo/releases/latest/download/latest.json`. For `/releases/latest/` to resolve, releases are no longer marked prerelease on GitHub. The Beta tag in the app replaces that flag.
- Bolo Nightly: `https://github.com/Nirav1810/bolo/releases/download/nightly/latest.json`.

The app checks on launch and every 6 hours, and never installs without a click. When a newer build is out, Settings → About and the tray offer **Update**. Clicking it downloads the installer, checks its minisign signature against the public key compiled into the app, runs it quietly (per-user NSIS needs no elevation), and the installer restarts the app. About links to the other app's download page instead of a channel switch.

*Amended in M4 (ticket 05, update settings):* the check runs on launch and once a day, not every 6 hours, and `[updates] auto_check` (a switch in About, on by default, shared by both apps) turns the automatic checks off; **Check now** still works.

The installers are signed with a Tauri updater key (minisign). The private key and its password live in the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, with an offline copy kept by the maintainer. Losing the key strands every install on its current version, because a new key needs a manual reinstall. Authenticode signing is a separate decision (M4).

## Considered Options

- **One app with a channel picker** (first version of this ADR): rejected at first. The user wanted to see at a glance which build runs. One install that changes its name and icon on update leaves Windows shortcuts and the uninstall entry behind the rename. *Taken in the 2026-11-13 amendment:* the product name no longer changes, so the shortcuts and uninstall entry stay "Bolo". The build that runs still shows its channel through its icon, title and pill.
- **Keep stable + beta (`-beta.N`) and add nightly**: rejected. Three channels for a pre-1.0 app with one maintainer is more to cut and explain. "Beta until 1.0" carries the same meaning.
- **Hand-cut alpha tags instead of nightly**: rejected. Every merge already passes CI, and an automatic build is what the UI work needs for fast try-it-yourself loops.
- **Separate settings per app**: rejected. Trying a nightly shouldn't mean setting Bolo up again. The settings file is versioned and keeps unknown keys, so the two apps can share it across versions.
- **Fully automatic install**: rejected for now. Dictation is something you're in the middle of. Restarting under the user is worse than one click.
- **Auto-update on the CLI zip**: out of scope. The CLI is a developer tool.
