# Android FFI: one `Bolo` object, audio pushed from Kotlin, secrets in a Keystore-backed store

`bolo-ffi` exposes a narrow UniFFI surface (proc-macros, no UDL) built around one object, `Bolo`. Kotlin never sees `EngineConfig`, `Settings` or the model manifest as mutable Rust structs. It calls methods (`start_dictation(app)`, `stop`, `cancel`, `settings()`, `save_settings(form)`, `download_model(id)`, `history(...)`) and gets events through one foreign trait, `BoloListener` (`on_partial`, `on_final`, `on_polished`, `on_level`, `on_state`, `on_error`, `on_download`, `on_usage`). Every listener call comes from a Rust thread, so Kotlin posts it to the main thread. The settings file, history database and models are the desktop ones (`bolo-store`, `bolo-models`), and Kotlin passes in the directories (`filesDir`, `noBackupFilesDir/models`).

**Audio comes from Kotlin.** An `AudioRecord` using the `VOICE_RECOGNITION` source records 16 kHz mono PCM16 and pushes ~20 ms frames through `Bolo.push_audio(frame)` into a lock-free ring. The ring is the `AudioSource` the engine drains (the same seam `WavSource` uses). `bolo-audio`'s `capture` feature (cpal) is off on Android.

**Secrets.** Kotlin implements `SecretStore` (`get`, `set`, `delete`) as a foreign trait. Values are encrypted with an AES-GCM key held in the Android Keystore and stored in private SharedPreferences. `bolo-ffi` wraps the trait in a `keyring_core::CredentialStore`, so `bolo_store::Secrets::with_store` works unchanged.

**TLS.** Cloud engines use `rustls-platform-verifier`. On Android it needs its Kotlin component (the Maven artifact shipped inside the crate) and a one-time `init_with_env` from JNI. `Bolo`'s constructor does that from the `Context` Kotlin passes in.

## Considered Options

- **cpal (AAudio) for the mic:** rejected, even though it would share the desktop code. cpal can't set the input preset, and `VOICE_RECOGNITION` is what turns off the call-style processing that hurts recognition. Kotlin also owns the permission, Bluetooth SCO routing and the IME lifecycle, so the mic belongs there. The push-frame cost (~50 FFI calls/s) is negligible.
- **Expose the engine and store as separate objects:** rejected. Kotlin would have to repeat desktop's `Controller` logic (config from settings + secrets, `set_polish` per dictation, snippets, history journaling). One object keeps that logic in Rust and shared.
- **Keys in `settings.toml` or plain SharedPreferences:** rejected. The keys are paid credentials. App-private storage stops other apps, but not backups or a rooted phone.
- **`android-keyring` style crates using JNI from Rust:** rejected for now. A Kotlin implementation of a three-method trait is simpler to test than JNI calls into the Keystore.
