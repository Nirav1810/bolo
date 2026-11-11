# 03: In-app updater: channel, checks, About and tray

**What to build:** the app side of ADR-0019.

**Blocked by:** 01 (the public key and the feed format)

**Status:** ready-for-agent

- [ ] `bolo-store`: `[updates] channel = "beta" | "nightly"`. The default comes from the build: a nightly build defaults to `nightly`, everything else to `beta`. Unknown keys are kept, as in every section.
- [ ] `updates.rs` is a pure module, unit-tested:
  - which channel a version belongs to and its badge (`Nightly` / `Beta` while 0.x / none);
  - the feed URL for a channel;
  - when the next check is due (launch, then every 6 h);
  - whether an offered version is newer (no downgrade).
- [ ] `tauri-plugin-updater` builds the updater at runtime with the channel's endpoint. The check runs on a background task. Install uses NSIS `quiet` mode, then restarts the app. A running dictation is cancelled first.
- [ ] Commands `update_status`, `check_update` and `install_update`. An `AppEvent::Update` carries the state (idle / checking / available{version, notes} / downloading{pct} / ready / error) to the settings window.
- [ ] About: version plus badge, Channel picker, Check for updates, the available update with notes and an **Update** button, and download progress. This replaces the "doesn't update itself" row.
- [ ] Tray: an "Update to X…" item appears only when an update is available. The tooltip shows the version and the badge.
- [ ] Clippy and the tests are clean, and About is checked visually in headless Edge with a mocked `__TAURI_INTERNALS__`.
