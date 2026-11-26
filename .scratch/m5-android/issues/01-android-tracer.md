# 01: Android tracer: from the Gradle project to a dictation on the emulator

**What to build:** the thinnest end-to-end path. A Gradle project in `apps/android` loads `libbolo_ffi.so` through UniFFI and dictates with a local model on the emulator, and a debug activity shows the text. CI builds the debug APK.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `apps/android`: a Gradle (Kotlin DSL) project with the wrapper committed. Kotlin + Jetpack Compose, `minSdk 28`, `targetSdk`/`compileSdk 37` (the installed platform), package `dev.bolo.android`, ABIs `arm64-v8a` and `x86_64` (the emulator; debug builds only, release stays arm64). `versionName` comes from the workspace version in `Cargo.toml` (ADR-0013); `versionCode` is derived from it.
- [ ] Native build: `scripts/build-android.py [--release] [--abi ...]` runs `cargo ndk -P 28 -t <abi> build -p bolo-ffi` into `apps/android/app/src/main/jniLibs/<abi>/`, copies sherpa's `libsherpa-onnx-c-api.so` and `libonnxruntime.so` next to it, and generates the Kotlin bindings (`uniffi-bindgen` from the built `.so`) into the app's source set. Gradle calls it from a `preBuild` task, so `./gradlew assembleDebug` is the single command.
- [ ] sherpa libs: `scripts/fetch-sherpa-libs.py` learns `--android` and unpacks `jniLibs/<abi>` into `.cache/sherpa-lib-android/<abi>` (keeping the Windows `.cache/sherpa-lib` intact). The build script sets `SHERPA_ONNX_LIB_DIR` per ABI, which overrides `.cargo/config.toml`. (Probe on 2026-11-26: with that one env var, `bolo-ffi` and `bolo-store` already build and link for `aarch64-linux-android`.)
- [ ] `bolo-ffi` minimal surface (ADR-0023): `Bolo::new(models_dir, listener)`, `push_audio(Vec<i16>)`, `start()`, `stop()`, `cancel()`; the `BoloListener` foreign trait with `on_partial`, `on_final`, `on_level`, `on_state` and `on_error`. A `PushSource` ring implements `AudioSource`. `bolo-core` gains a `capture` feature (default on) so Android builds skip cpal.
- [ ] Debug activity: mic permission, an `AudioRecord` (`VOICE_RECOGNITION`, 16 kHz mono) pushing 20 ms frames, a hold-to-talk button, and a text view showing partials and then the final text. The model is the one in `filesDir/models` (pushed with `adb push` for now; ticket 05 does downloads).
- [ ] Emulator: install `system-images;android-37;google_apis;x86_64` and `emulator` with `sdkmanager`, then create an AVD (`bolo-api37`, 6 GB RAM). Document this in `apps/android/README.md`, along with routing the host mic into the AVD (`hw.audioInput=yes`; *Extended controls › Microphone › Virtual microphone uses host audio input*).
- [ ] CI: an `android (ubuntu-latest)` job runs fetch-sherpa-libs `--android`, `cargo ndk` for both ABIs, clippy for `aarch64-linux-android` on `bolo-ffi`, and `./gradlew assembleDebug lint`, then uploads the APK as an artifact. Make it a required check in the ruleset once it's green on main.
- [ ] Tests: `bolo-ffi` unit tests (host target) for `PushSource` (frames in → mono f32 out, overflow counted) and for a fake-STT dictation through the `Bolo` object, using the `bolo-core` test seam.

## Human: dictate on the emulator

- [ ] With the AVD using the host mic: open the debug activity, hold the button, say "testing one two three", and release. The partials appear, then the final text.
