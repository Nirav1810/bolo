# 04: Settings, history and secrets over FFI

**What to build:** `bolo-ffi` exposes the desktop's store on Android, so the settings app and the IME read and write the same `settings.toml` v2, history and keys.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] `bolo-store` builds for Android: paths are injected (`Paths { config_dir, data_dir, models_dir }`) and don't come from `directories`; the Windows keyring store stays `cfg(windows)`.
- [ ] `Bolo::new(paths, secret_store, listener)`. `SecretStore` is a foreign trait (`get`/`set`/`delete`) wrapped as a `keyring_core::CredentialStore` (ADR-0023). Kotlin implements it with a Keystore AES-GCM key and private SharedPreferences.
- [ ] Settings over FFI as plain records: read, save (validated the same way as desktop's `form.rs`; share that code by moving it into `bolo-store` if it's platform-neutral), and an event when the file changes (the IME and settings activity both hold a `Bolo`).
- [ ] `start_dictation(package_name, field)`: per-app resolution by package name (ADR-0022; `[[apps]]` rows match `exe` or `package`) and bundled Android defaults (`app_rules.toml` gains Android rows: WhatsApp, Gmail, Chrome, Keep, Messages, Slack). Sensitive fields → no recording, no history.
- [ ] History: the journaling desktop does (text only, 30 days), list/search/delete/clear over FFI.
- [ ] Tests: settings round-trip through the FFI records, package-name resolution, and secrets against keyring's mock store. Kotlin unit tests for the Keystore `SecretStore` run as an instrumented test on the emulator (`connectedDebugAndroidTest`, local only; CI runs the JVM tests).
