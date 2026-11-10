# 02: bolo-store: settings, history, secrets, dictionary

**What to build:** The `bolo-store` crate: a versioned settings TOML that maps onto `EngineConfig`, SQLite history with a 30-day prune, API keys in Windows Credential Manager, and the user dictionary.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `Settings` at `%APPDATA%\Bolo\settings.toml`, with `version = 1` and a migration hook. Fields:
  - engines, language mode, Hinglish script;
  - the push-to-talk key;
  - polish on/off, tone, per-app overrides;
  - the history toggle and retention days (default 30);
  - autostart;
  - the dictionary.
- [ ] Unknown fields survive a load and save. A file that doesn't parse is renamed to `settings.toml.bad-<time>` and replaced with defaults.
- [ ] `Settings::engine_config(&Secrets) -> EngineConfig`. The dictionary feeds both `EngineConfig::dictionary` and polish's `dictionary`.
- [ ] History in `%LOCALAPPDATA%\Bolo\history.sqlite` (`rusqlite`, bundled). Each row stores the time, app, language, engine, final text, polished text, swap outcome and timings JSON. Search by substring, list the latest N, prune older than N days, and clear (delete + vacuum). Turning history off stops writes and doesn't delete existing rows.
- [ ] `Secrets` through `keyring` (Credential Manager, service `bolo`, one entry per provider), loaded into `ApiKeys`. Keys never appear in `Debug`, logs or the TOML.
- [ ] Tests: settings round trip, migration from a v0 fixture, recovery from a bad file, history insert/search/prune/clear on a temp DB, secrets on keyring's mock store.
