# Per-app profiles: the user's rows win over the bundled app rules

The original spec asks for "app→format and app→tone mapping is a user-editable table with sensible defaults". In M3 the bundled `app_rules.toml` (ADR-0018) decides format, code mode, paste chord and multi-line swap safety, and the user can only turn polish on or off per exe (`[polish.apps]`).

M4 adds a **user profile table**, `[[apps]]` in `settings.toml`. Each row has an `exe`, plus optional `name`, `polish`, `tone` and `format`. The fields are resolved one by one: a field the user set wins, otherwise the bundled rule's field applies, otherwise the default does (plain, global polish switch, global tone). Code mode, the paste chord and swap safety stay bundled-only. They describe how an app behaves rather than what the user prefers, and a wrong value breaks insertion. Settings go to version 2, and the migration turns `[polish.apps]` into `[[apps]]` rows.

For browsers, a bundled title rule (Gmail) still matches first for that tab. A user row for `chrome.exe` applies to every other tab, and to the fields the title rule leaves unset.

## Considered Options

- **An editable copy of the whole `app_rules.toml`:** rejected. A user's copy would freeze the bundled fixes that later releases ship, and it exposes fields that break insertion.
- **Per-app tone only:** rejected. The spec asks for format too, and Slack or Notion users want Markdown when the bundled default is plain.
