# 03: In-app updater: channel, checks, About and tray

**What to build:** the app side of ADR-0019.

**Blocked by:** 01 (the public key and the feed format)

**Status:** done

- [x] `bolo-store`: `[updates] channel = "beta" | "nightly"`. The default comes from the build: a nightly build defaults to `nightly`, everything else to `beta`. Unknown keys are kept, as in every section.
- [x] `updates.rs` is a pure module, unit-tested:
  - which channel a version belongs to and its badge (`Nightly` / `Beta` while 0.x / none);
  - the feed URL for a channel;
  - when the next check is due (launch, then every 6 h);
  - whether an offered version is newer (no downgrade).
- [x] `tauri-plugin-updater` builds the updater at runtime with the channel's endpoint. The check runs on a background task. Install uses NSIS `quiet` mode, then restarts the app. A running dictation is cancelled first.
- [x] Commands `update_status`, `check_update` and `install_update`. An `AppEvent::Update` carries the state (idle / checking / available{version, notes} / downloading{pct} / ready / error) to the settings window.
- [x] About: version plus badge, Channel picker, Check for updates, the available update with notes and an **Update** button, and download progress. This replaces the "doesn't update itself" row.
- [x] Tray: an "Update to X…" item appears only when an update is available. The tooltip shows the version and the badge.
- [x] Clippy and the tests are clean, and About is checked visually in headless Edge with a mocked `__TAURI_INTERNALS__`.

## Outcome

- `bolo-store`: `[updates] channel` (`UpdateChannel::{Beta, Nightly}`), absent until the user picks one.
- `updates.rs`:
  - pure rules: `badge`, `build_channel`, `feed`, `is_newer` and the 20 s / 6 h timing;
  - `Updates`: the status, plus `offer()`, which drives the tray;
  - `check()` and `install()` run the plugin with the channel's endpoint, a no-downgrade comparator and an `on_before_exit` that unhooks the keyboard and drops the engine.
- A failed quiet re-check keeps an already-found update. A failed download leaves the tray offer up, so it can be retried.
- `tauri-plugin-updater` builds without its `zip` feature: four RUSTSEC zip advisories, and we never serve zipped bundles.
- Commands: `update_view`, `update_check`, `update_install`, `update_channel` (saves the channel and checks it at once).
- Tray: "Restart to update to X" is inserted at the top while an update is offered and installs on click. The tooltip is `Bolo 0.4.0 Beta`.
- About: version with a Beta/Nightly tag; an Updates group with status, Check / Update now, download progress and the notes; a Channel picker; the files moved under "Files". Checked in headless Edge with a mocked `__TAURI_INTERNALS__` (available, downloading, up to date).
- Not yet exercised against a real feed: ticket 04.
