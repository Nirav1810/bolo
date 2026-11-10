# 02: bolo-store: settings, history, secrets, dictionary

**What to build:** The `bolo-store` crate: a versioned settings TOML that maps onto `EngineConfig`, SQLite history with a 30-day prune, API keys in Windows Credential Manager, and the user dictionary.

**Blocked by:** none

**Status:** done

- [x] `Settings` at `%APPDATA%\Bolo\settings.toml`, with `version = 1` and a migration hook. Fields:
  - engines, language mode, Hinglish script;
  - the push-to-talk key;
  - polish on/off, tone, per-app overrides;
  - the history toggle and retention days (default 30);
  - autostart;
  - the dictionary.
- [x] Unknown fields survive a load and save. A file that doesn't parse is renamed to `settings.toml.bad-<time>` and replaced with defaults.
- [x] `Settings::engine_config(&Secrets) -> EngineConfig`. The dictionary feeds both `EngineConfig::dictionary` and polish's `dictionary`.
- [x] History in `%LOCALAPPDATA%\Bolo\history.sqlite` (`rusqlite`, bundled). Each row stores the time, app, language, engine, final text, polished text, swap outcome and timings JSON. Search by substring, list the latest N, prune older than N days, and clear (delete + vacuum). Turning history off stops writes and doesn't delete existing rows.
- [x] `Secrets` through `keyring` (Credential Manager, service `bolo`, one entry per provider), loaded into `ApiKeys`. Keys never appear in `Debug`, logs or the TOML.
- [x] Tests: settings round trip, migration from a v0 fixture, recovery from a bad file, history insert/search/prune/clear on a temp DB, secrets on keyring's mock store.

## Notes

- `settings.rs`: `Settings` (with `PushToTalkKey`, `PolishSettings`, `HistorySettings`) at `%APPDATA%\Bolo\settings.toml`. `Settings::load` returns `Loaded` with `recovered_from` (the `settings.toml.bad-<unix secs>` backup) and `migrated_from`. A migrated file is saved straight away. A file from a newer Bolo returns `StoreError::NewerSettings` and is left alone. Saves go through a temp file and a rename.
- Version 0 is the flat layout of ticket 01's `BOLO_*` variables (`polish = "off"|"on"|"<tone>"`, `history`, `history_days`). Fixture: `crates/bolo-store/tests/fixtures/settings-v0.toml`.
- `Settings::engine_config(&Secrets)`: the dictionary is trimmed and de-duplicated, then fed to `EngineConfig::dictionary` and `Polish::On { dictionary }`. Polish is on only when it's enabled *and* the Anthropic key is stored. Its `app` is left empty for the app to fill per dictation. An empty engine list falls back to the default, and `models_dir` falls back to the ADR-007 location.
- `history.rs`: `History::open`, `insert` (returns `None` while off), `record_polish(id, polished, swap)` (polish arrives after the insert), `get`, `latest`, `search` (LIKE with `%`/`_` escaped, over the final and polished text), `prune(days, now_ms)`, `clear` (DELETE + VACUUM; a test checks the text is gone from the file), `set_enabled`. The schema version lives in `PRAGMA user_version`. The app does the prune at startup and daily (ticket 07).
- `secrets.rs`: keyring v4 is split, so this uses `keyring-core` (API and mock store) plus `windows-native-keyring-store` on Windows. `Secrets::os()` / `Secrets::with_store(store)` load `PROVIDERS` (deepgram, groq, sarvam, anthropic) from service `bolo`. `set` with a blank key deletes the entry. An unreadable entry is logged by provider name and skipped. Errors carry only the store's `Display` text, which never includes the secret. An ignored test round-trips a throwaway entry in the real Credential Manager; it passed on the dev machine.
- The desktop app still reads `BOLO_*` variables. Wiring it to the store belongs to tickets 07 and 09.
- `cargo deny`: passes with no new ignores. SQLite is bundled (`rusqlite` `bundled`).
