# 01: Apps tab: per-app polish, tone and format

**What to build:** ADR-0020. One user table of per-app profiles that wins over the bundled `app_rules.toml`, edited in a new Apps tab.

**Blocked by:** none

**Status:** done

- [x] `bolo-store`: settings version 2. `[[apps]]` entries have `exe` (lower-case image name) and optional `name`, `polish` (bool), `tone` (string) and `format` (`plain | markdown | html`). The v1→v2 migration moves `[polish.apps]` (exe → bool) into `[[apps]]` rows, and is tested like v0→v1.
- [x] `rules.rs`: `resolve`/`plan` take the user profiles. A field the user set wins over the bundled rule's field. Unset fields fall through to the rule, then to the defaults. Tone falls back to the global tone. A user row for a browser exe applies to the whole browser, but a bundled title rule (Gmail) is still checked first for that tab. Unit-tested.
- [x] Commands to list rows (bundled ∪ user, with each field's effective value and where it came from), set a field, reset a row and add an app. The most recent foreground exe that isn't Bolo is offered for **Add app**.
- [x] `src/settings/Apps.svelte` is the new tab, in the existing T3 Code style (`lib/ui`). Polish → Per app is removed and Polish keeps the global switch and tone. Checked visually in headless Edge with a mocked `__TAURI_INTERNALS__`.
- [x] Clippy, tests and `npm run check` are clean.

## Outcome

Merged as PR #60 (7485bb6). Settings v2 with `[[apps]]` (exe plus optional name, polish, tone and format) and a v1→v2 migration from `[polish.apps]`. `rules::resolve`/`plan` take `&[AppProfile]` and resolve each field: user, then bundled, then default. The commands are `app_set`, `app_reset` and `app_add`, and the `SettingsView.apps` rows show where each value comes from. A row for one exe in a bundled group (Terminal) applies to the whole group unless another member has its own row. The Apps tab was checked in headless Edge, light and dark. The live app checks are in the M4 report's checklist.
