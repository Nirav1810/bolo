# Updates: in-app updater with Beta and Nightly channels

**Why:** the next work is a large UI refinement, and you should be able to try each merged change from inside the app without reinstalling by hand. Today Bolo can't update itself, and About only links to the releases page. Decisions are in ADR-0019. This comes before the UI work and before M4.

## User-facing behaviour

- Settings → About shows `Bolo 0.4.0` with a **Beta** or **Nightly** badge, plus a **Channel** picker (Beta / Nightly).
- On launch and every 6 h, Bolo checks the channel's feed. If a newer build exists:
  - About shows "Version 0.4.1 is available" with release notes and an **Update** button.
  - The tray menu gets an "Update to 0.4.1…" item.
- **Update** downloads with progress, verifies the signature, installs quietly and restarts Bolo. Settings stay as they are, and a dictation in progress is cancelled first.
- **Check for updates** runs a check now and reports "You're up to date", "Update available" or the error.
- Switching to Nightly checks right away. Switching back to Beta doesn't downgrade.
- If a check fails (offline, feed down), it fails quietly. The error shows only on a manual check.

## Out of scope

- Authenticode signing (M4).
- Delta updates.
- Updating the CLI zip.
- Android.
- Forced updates and minimum versions.

## Tickets

1. `01-signed-release-feed`: signing key, updater artifacts and `latest.json` on releases; 0.x releases stop being prereleases.
2. `02-nightly-builds`: a nightly workflow on every push to `main`, publishing to the rolling `nightly` release.
3. `03-in-app-updater`: plugin, channel setting, checks, About page and tray.
4. `04-cut-v0.4.0`: release, then end-to-end checks of nightly → nightly and v0.4.0 → nightly.
