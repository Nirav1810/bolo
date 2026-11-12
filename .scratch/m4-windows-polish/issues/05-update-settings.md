# 05: Update settings

**What to build:** the original spec's "checks on startup + every 24 h (user can disable)".

**Blocked by:** none

**Status:** done

- [x] `bolo-store`: `[updates] auto_check = true`.
- [x] `updates.rs`: the interval becomes 24 h. With auto_check off, no background checks run, but a manual **Check for updates** still does. Unit-tested.
- [x] About gets an "Check for updates automatically" switch. Checked visually.
- [x] Clippy, tests and `npm run check` are clean.

## Outcome

- `bolo-store`: `[updates] auto_check` (`UpdateSettings`, default on, unknown keys kept). Additive: a file without `[updates]` loads with checks on, so no version bump. Round-trip and default tests.
- `updates.rs`: `CHECK_EVERY` is 24 h. The pure `check_due(auto_check, since_last)` decides each background check; the schedule in `lib.rs` waits 20 s after launch, then asks it every `SCHEDULE_TICK` (10 min), reading the switch from the live settings each time. Off: no launch check and no daily check. Back on: a check within 10 minutes, then daily, no restart. Manual **Check now** never goes through `check_due`. Tests simulate the schedule tick by tick (always on, always off, launched off then switched on, switched off and back on).
- The window edits it as `Form.auto_update_check`; saving needs nothing reapplied, since the schedule reads it at the next tick.
- About: a "Check for updates automatically" switch in the Updates group. With it off and nothing checked yet, the status row reads "Automatic updates are off / Check now to look for an update." with a muted icon. Checked in headless Edge, light and dark, on and off (`shots/about-updates-*.png` in the session scratchpad).
- Bolo Nightly shares `settings.toml` with Bolo, so the switch covers both apps; the row says so.
- README (privacy notes) and ADR-0019 (a one-line amendment) now say "once a day" and that it can be turned off. The finished `updates` tickets and spec keep their "every 6 h" as history.
