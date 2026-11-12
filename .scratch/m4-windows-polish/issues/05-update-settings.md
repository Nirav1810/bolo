# 05: Update settings

**What to build:** the original spec's "checks on startup + every 24 h (user can disable)".

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `bolo-store`: `[updates] auto_check = true`.
- [ ] `updates.rs`: the interval becomes 24 h. With auto_check off, no background checks run, but a manual **Check for updates** still does. Unit-tested.
- [ ] About gets an "Check for updates automatically" switch. Checked visually.
- [ ] Clippy, tests and `npm run check` are clean.
