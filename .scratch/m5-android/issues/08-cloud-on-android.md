# 08: Cloud engines and Claude polish on Android

**What to build:** BYOK cloud engines and polish work on the phone as they do on desktop.

**Blocked by:** 04

**Status:** ready-for-agent

- [ ] TLS: include `rustls-platform-verifier-android`'s Kotlin component (the crate ships a Maven repo; point Gradle at it via `cargo metadata`), and call `rustls_platform_verifier::android::init_with_env` once. UniFFI uses JNA, so it goes through a small exported JNI function in `bolo-ffi` (`Java_dev_bolo_android_Native_initTls(env, _, context)`) that Kotlin calls before `Bolo::new`.
- [ ] Deepgram (stream and batch), Groq, Sarvam and Claude polish run on the emulator with real keys, through `bolo dictate`'s code paths. `on_usage` feeds the usage table, and a Usage section in settings shows it (as desktop).
- [ ] Network changes: a dictation that loses the network falls back down the engine chain (`FallbackEngine`), and the panel says "offline, using the local model" once.
- [ ] Tests: the JNI init is idempotent (called twice → ok), and a TLS handshake to a real host from an instrumented test (local only).

## Human: keys on the emulator

- [ ] Set a Deepgram key, dictate Hinglish, and see it in Usage. Turn on airplane mode and dictate again: the local model takes over and the notice shows.
