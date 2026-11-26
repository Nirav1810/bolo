# 01: Android tracer: from the Gradle project to a dictation on the emulator

**What to build:** the thinnest end-to-end path. A Gradle project in `apps/android` loads `libbolo_ffi.so` through UniFFI and dictates with a local model on the emulator, and a debug activity shows the text. CI builds the debug APK.

**Blocked by:** none

**Status:** ready-for-human (speak into the emulator; the `android` CI check becomes required once it's green on main)

- [x] `apps/android`: a Gradle (Kotlin DSL) project with the wrapper committed. Kotlin + Jetpack Compose, `minSdk 28`, `targetSdk`/`compileSdk 37` (the installed platform), package `dev.bolo.android`, ABIs `arm64-v8a` and `x86_64` (the emulator; debug builds only, release stays arm64). `versionName` comes from the workspace version in `Cargo.toml` (ADR-0013); `versionCode` is derived from it.
- [x] Native build: `scripts/build-android.py [--release] [--abi ...]` runs `cargo ndk -P 28 -t <abi> build -p bolo-ffi` into `apps/android/app/src/main/jniLibs/<abi>/`, copies sherpa's `libsherpa-onnx-c-api.so` and `libonnxruntime.so` next to it, and generates the Kotlin bindings (`uniffi-bindgen` from the built `.so`) into the app's source set. Gradle calls it from a `preBuild` task, so `./gradlew assembleDebug` is the single command.
- [x] sherpa libs: `scripts/fetch-sherpa-libs.py` learns `--android` and unpacks `jniLibs/<abi>` into `.cache/sherpa-lib-android/<abi>` (keeping the Windows `.cache/sherpa-lib` intact). The build script sets `SHERPA_ONNX_LIB_DIR` per ABI, which overrides `.cargo/config.toml`. (Probe on 2026-11-26: with that one env var, `bolo-ffi` and `bolo-store` already build and link for `aarch64-linux-android`.)
- [x] `bolo-ffi` minimal surface (ADR-0023): `Bolo::new(models_dir, listener)`, `push_audio(Vec<i16>)`, `start()`, `stop()`, `cancel()`; the `BoloListener` foreign trait with `on_partial`, `on_final`, `on_level`, `on_state` and `on_error`. A `PushSource` ring implements `AudioSource`. `bolo-core` gains a `capture` feature (default on) so Android builds skip cpal.
- [x] Debug activity: mic permission, an `AudioRecord` (`VOICE_RECOGNITION`, 16 kHz mono) pushing 20 ms frames, a hold-to-talk button, and a text view showing partials and then the final text. The model is the one in `filesDir/models` (pushed with `adb push` for now; ticket 05 does downloads).
- [x] Emulator: install `system-images;android-37;google_apis;x86_64` and `emulator` with `sdkmanager`, then create an AVD (`bolo-api37`, 6 GB RAM). Document this in `apps/android/README.md`, along with routing the host mic into the AVD (`hw.audioInput=yes`; *Extended controls › Microphone › Virtual microphone uses host audio input*).
- [x] CI: an `android (ubuntu-latest)` job runs fetch-sherpa-libs `--android`, `cargo ndk` for both ABIs, clippy for `aarch64-linux-android` on `bolo-ffi`, and `./gradlew assembleDebug lint`, then uploads the APK as an artifact. Make it a required check in the ruleset once it's green on main.
- [x] Tests: `bolo-ffi` unit tests (host target) for `PushSource` (frames in → mono f32 out, overflow counted) and for a fake-STT dictation through the `Bolo` object, using the `bolo-core` test seam.

## Human: dictate on the emulator

- [ ] With the AVD using the host mic: open the debug activity, hold the button, say "testing one two three", and release. The partials appear, then the final text.

## Outcome

- **Toolchain.** AGP 9.5.3 (built-in Kotlin), Kotlin 2.4.21 with the Compose compiler plugin, Gradle 9.9.2 (wrapper committed), Compose BOM 2026.11.01, and JNA 5.19.1 for UniFFI 0.33. `compileSdk` is **37.2**, not 37, because AndroidX core 1.19 and Compose 1.13 refuse to compile against less than 37.1. `targetSdk` is **36**: there's no API 37 emulator image yet, so the AVD is `bolo-api36`, `system-images;android-36;google_apis_playstore;x86_64`. The Play Store image is there so Gmail and Chrome are available for the acceptance run. `versionCode` is `MMmmpp00` from the workspace version (0.5.1 → 50100).
- **Build.** `scripts/build-android.py` writes to `app/build/rustJniLibs/<abi>/` and `app/build/generated/uniffi/`, which are git-ignored build outputs, not `src/main/jniLibs`. Gradle wires them in with `jniLibs.directories` / `kotlin.directories`, through `cargoBuildDebug`/`cargoBuildRelease` Exec tasks hooked to `pre{Debug,Release}Build`. A no-op rebuild takes a few seconds. All three `.so` files are 16 KB page-aligned (LOAD align `0x4000`, checked with `llvm-readelf`), as Android 15+ 16 KB-page devices require. The debug APK is 141 MB: two ABIs of unoptimized, stripped `libbolo_ffi.so` (21 and 29 MB) plus onnxruntime (22 and 26 MB), stored uncompressed. Release builds are arm64 only and optimized.
- **FFI.** `push_audio` takes **`Vec<u8>` little-endian PCM16**, not `Vec<i16>`. UniFFI maps it to a `ByteArray` that `AudioRecord.read` fills directly, where `List<Short>` would box 16 000 samples a second. `PushRing` drops frames while no `PushSource` is open, keeps the newest 4 s and counts overflow. Errors are `FfiException.Engine(kind, detail)`; a field can't be called `message`, which clashes with `Throwable.message`. `uniffi.toml` sets `android = true`, so the bindings use `SystemCleaner` behind an `SDK_INT >= 33` check. Lint caught `java.lang.ref.Cleaner` (API 33) in the default bindings. `bolo-core`'s workspace dependency is now `default-features = false`, and the CLI and desktop opt into `capture`. `Bolo::with_stt` (`#[doc(hidden)]`) injects a fake engine for tests. tracing goes to logcat (`tracing-android`, tag `bolo`).
- **Debug replay.** Debuggable builds take `am start ... --es replay <wav> [--es engine <spec>]`. That pushes a 16 kHz WAV through `pushAudio` in real time and logs `replay text [LANG]: ...`, so dictations run on the emulator with no one speaking. On the emulator (debug build, x86_64 WHPX, 4 vCPU):

  | Engine | Clip | Text | Release → text | Model load |
  | --- | --- | --- | --- | --- |
  | moonshine-tiny | en/02 | "And you send me the report by Friday." | 3–8 ms (VAD segments decode while speaking) | 3.2–4.5 s |
  | nemotron-stream (Auto, dual-pass) | en/02 | "Can you send me the report by Friday." → English | 1211 ms | 58 s cold, 11 s with a warm page cache |
  | nemotron-stream (Auto, dual-pass) | hinglish/01 | "Haan, theek hai, kal milta hai." → Hinglish | 1811 ms | 11 s |

  The model unloads after the 20 s idle timeout, as designed. Ticket 09 measures properly, on a release build.
- **Emulator facts.** With `hw.ramSize=6144M` the guest reports `MemTotal` 6 072 060 kB (5.79 GiB, 5930 MiB). A phone sold as "6 GB" reports less than 6000 MiB too, so the `android-high` threshold of 6000 in the manifest would put real 6 GB phones in the low tier. Ticket 05 has to fix the threshold.
- **CI.** The `android (ubuntu-latest)` job runs clippy for `aarch64-linux-android`, then `assembleDebug lintDebug testDebugUnitTest` (Gradle builds both ABIs through the script), and uploads the APK artifact. Lint is clean apart from three warnings: OldTargetApi (36), ChromeOS x86_64 in release, and `allowBackup` → `dataExtractionRules` (M6).
