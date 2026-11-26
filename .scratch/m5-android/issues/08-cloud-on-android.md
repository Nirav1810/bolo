# 08: Cloud engines and Claude polish on Android

**What to build:** BYOK cloud engines and polish work on the phone as they do on desktop.

**Blocked by:** 04

**Status:** done (Human check owed)

- [x] TLS: include `rustls-platform-verifier-android`'s Kotlin component (the crate ships a Maven repo; point Gradle at it via `cargo metadata`), and call `rustls_platform_verifier::android::init_with_env` once. UniFFI uses JNA, so it goes through a small exported JNI function in `bolo-ffi` (`Java_dev_bolo_android_Native_initTls(env, _, context)`) that Kotlin calls before `Bolo::new`.
- [x] Deepgram (stream and batch), Groq, Sarvam and Claude polish run on the emulator with real keys, through `bolo dictate`'s code paths. `on_usage` feeds the usage table, and a Usage section in settings shows it (as desktop). *(Groq and Sarvam with bogus keys only: there are no real keys for them. The Usage section is an FFI call plus a debug line; the page is ticket 07's.)*
- [x] Network changes: a dictation that loses the network falls back down the engine chain (`FallbackEngine`), and the panel says "offline, using the local model" once.
- [x] Tests: the JNI init is idempotent (called twice → ok), and a TLS handshake to a real host from an instrumented test (local only).

## Human: keys on the emulator

- [ ] Set a Deepgram key, dictate Hinglish, and see it in Usage. Turn on airplane mode and dictate again: the local model takes over and the notice shows.

## Outcome

- **The Kotlin component isn't in the crate.** `rustls-platform-verifier-android` 0.2.0 on crates.io holds only `src/lib.rs`: the AAR moved to a Maven repository on the rustls GitHub (`maven-archive` branch), which the crate's README now points at. So `settings.gradle.kts` adds that repository with `exclusiveContent` (only the `org.rustls` group resolves there), and `app/build.gradle.kts` reads the crate's version from `Cargo.lock` and pins `org.rustls:rustls-platform-verifier` to it (`strictly`). No `cargo metadata`: the lockfile is enough now that the crate's source isn't needed. The AAR has no dependencies. CI's android job needed no change.
- **`Native.init(context)`.** `crates/bolo-ffi/src/tls.rs` exports `Java_dev_bolo_android_Native_initTls` (jni 0.22's `jni_mangle`, already in the tree through the verifier; `cargo deny` passes). It calls `init_with_env` with the application context and sets a ready flag; a second call returns at once, and the verifier keeps its first context anyway. Kotlin's `object Native` loads `bolo_ffi` with `System.loadLibrary` (the same library JNA opens, so one copy of the statics) and exposes `init(context)`. **`Bolo::new` refuses to open on Android until it has run** (`FfiException.Engine(CONFIG, "TLS isn't set up: ...")`): without it, the first connection panics inside the verifier, and release builds are `panic = "abort"`. ADR-0023's TLS paragraph says so.
- **`probe_https(url) -> status`** (a free function) makes one GET with the engines' own client, for the instrumented test and future connection checks. A refused certificate or no network is the new `FfiException.Network(detail)`, with the error's causes spelled out.
- **Usage.** `bolo-store` gained `History::usage_summary(today_start_ms, now_ms) -> UsageSummary` (today, 30 days, all time, since), and the desktop's `usage_summary` command uses it instead of its own `UsageView` (same JSON). `Bolo.usage_summary(today_start_ms)` returns it as `UsageSummary { today, last_30_days, all_time: List<ProviderTotals>, since_ms }`; `Bolo.reset_usage()` empties it. The phone passes its local midnight, as the desktop window does.
- **Offline notice.** `BoloListener.on_notice(Notice)` with one variant, `Notice.OFFLINE`, sent just before the `on_final` it is about when the chain starts with a cloud engine and the text came from a `local:` engine. It's said once, and said again only after a cloud engine has worked in between. A local-first chain never says it. Kotlin owns the wording ("Offline, using the local model").
- **Debug screen.** `Native.init` in `onCreate`; the engine field takes a comma-separated chain; a usage line (today and 30 days per provider); `onNotice` shows in the status line; `onError` is logged. Debuggable builds take `--es import_keys <file>` (`provider=key` lines in `filesDir`, stored through the Keystore, then the file is deleted; a PowerShell pipe's BOM is stripped) and `--ez polish true|false`. The README shows how to pipe keys in without them touching a command line.
- **On the emulator** (bolo-api36, debug build, x86_64):

  | Check | Result |
  | --- | --- |
  | Deepgram batch, real key, hinglish/01 | "Haan theek hai, kal milta hai." in 501 ms warm (3.6 s on the first connection); metered |
  | Deepgram stream, real key, en/02 | "Can you send me the report by Friday?" 246–326 ms release → text; metered |
  | Claude polish, real key | polished and metered (`anthropic` rows) when warm; the first polish of a cold process missed the 1.5 s deadline and kept the Layer 1 text |
  | Groq, Sarvam, bogus keys | "authentication failed (check the API key)" from the real hosts: TLS works, the auth error path works |
  | `deepgram-stream,local:moonshine-tiny`, wifi + data off before the dictation | DNS failure → moonshine-tiny text, `notice: OFFLINE` once; a second offline dictation has no notice; back online → Deepgram again |
  | Same chain, network cut 1.5 s into the dictation | "connection abort" → moonshine-tiny text and the notice |
  | `TlsTest` (instrumented) | idempotent init; Deepgram, Groq, Sarvam, Anthropic, GitHub and Hugging Face answer; expired, wrong-host and self-signed badssl.com certificates refused |

- **For ticket 09: the first connection of a process is slow on the emulator.** A fresh HTTPS connection to Deepgram takes ~1.2 s (it's far away: ~0.9–1.4 s from the PC with curl too), but the first one in a process takes 2.2–3.9 s, so the verifier's JNI/TrustManager start-up plus the debug build add ~1–2.5 s. One cold Deepgram WebSocket connect hit `deepgram_stream`'s 3 s connect timeout (a dictation with a local fallback would then go local and say "offline" wrongly). Measure on a release build and a phone, then either open the connection at the first mic tap, when the mic warms up, or give Android a longer connect timeout.
- **Release (M6) needs R8 keep rules** for `org.rustls.platformverifier.**` (called only from Rust), JNA and `dev.bolo.ffi.**`; they're in the README. Debug and today's release builds don't minify.
- **Tests.** bolo-store: the summary's periods. bolo-ffi: `tls` (ready on hosts, a refused connection is `Network`, not a panic), and through `Bolo` with a fake cloud engine whose network can be switched off in a real `FallbackEngine` with a fake local engine: the offline notice once, again after recovery, never for a local-first chain; cloud dictations in the usage summary (local ones not metered), kept by Clear history, emptied by Reset. Kotlin: `TlsTest` (instrumented, local only).
