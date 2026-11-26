# apps/android (M5)

A Kotlin + Jetpack Compose voice keyboard (`InputMethodService`, ticket 06) that loads `libbolo_ffi.so` through UniFFI (ADR-0023). It targets minSdk 28 and ships arm64-v8a, plus x86_64 in debug builds for the emulator. The plan is in `.scratch/m5-android/`.

Today the app is a debug screen. Hold the button to dictate with the engine, or replay a WAV file. Since ticket 04, `Bolo` reads `filesDir/settings.toml` (the desktop's schema), records history in `filesDir/history.sqlite`, and keeps API keys in `KeystoreSecretStore` (AES-GCM, key in the Android Keystore). The engine field writes the settings' engine chain (comma-separated, primary first: `deepgram-stream,local:moonshine-tiny`). Since ticket 08, the cloud engines (Deepgram batch and stream, Groq, Sarvam) and Claude polish work, the bottom line shows the usage meter, and the status line shows "Offline, using the local model" when the local model stands in for a cloud engine.

## Build

What you need:

- **JDK 17.**
- **The Android SDK** with `platforms;android-37.2` and NDK `30.0.16248370`. Set `ANDROID_HOME`, or put `sdk.dir` in `apps/android/local.properties`.
- **Rust** with `rustup target add aarch64-linux-android x86_64-linux-android`, plus `cargo install cargo-ndk`.
- **Python 3.**

```sh
cd apps/android
./gradlew assembleDebug     # app/build/outputs/apk/debug/app-debug.apk
```

Gradle runs `scripts/build-android.py` before every build. That script does three things:

1. It builds `bolo-ffi` with cargo-ndk for each ABI against sherpa-onnx's Android libraries. The first run fetches and verifies those with `fetch-sherpa-libs.py --android`.
2. It copies `libbolo_ffi.so`, `libsherpa-onnx-c-api.so` and `libonnxruntime.so` to `app/build/rustJniLibs/<abi>/`.
3. It generates the Kotlin bindings (package `dev.bolo.ffi`) into `app/build/generated/uniffi/`.

Gradle also pulls `org.rustls:rustls-platform-verifier`, the Kotlin half of the TLS verifier, from the rustls project's Maven repository on GitHub (only the `org.rustls` group resolves there). Its version is the `rustls-platform-verifier-android` crate's in `Cargo.lock`, so the Kotlin and Rust halves always match: a `cargo update` that moves the crate moves the Kotlin component with it.

Run it by hand (`python scripts/build-android.py [--release] [--abi x86_64]`) to rebuild only the native side. Debug `.so` files are big because they have debuginfo, but AGP strips them in the APK. All three libraries are 16 KB page-aligned.

To use the `-p bolo-ffi` cargo commands for Android yourself, point `SHERPA_ONNX_LIB_DIR` at `.cache/sherpa-lib-android/<abi>`. `.cargo/config.toml` points it at the Windows libraries.

## Emulator

The `x86_64` image runs with WHPX on Windows. To keep the SDK, AVDs and Gradle caches off C:, this machine uses `ANDROID_HOME=E:\Android\Sdk`, `ANDROID_AVD_HOME=E:\Android\avd` and `GRADLE_USER_HOME=E:\.gradle`.

```sh
sdkmanager "emulator" "system-images;android-36;google_apis_playstore;x86_64"
avdmanager create avd -n bolo-api36 -k "system-images;android-36;google_apis_playstore;x86_64" -d pixel_7
# In the AVD's config.ini: hw.ramSize=6144M (the guest then reports ~5.8 GiB), hw.audioInput=yes
emulator -avd bolo-api36 -allow-host-audio      # add -no-window to run it headless
```

To speak into the emulator, open the emulator's *Extended controls › Microphone* and turn on *Virtual microphone uses host audio input*.

## TLS

Every HTTPS and WebSocket connection (cloud engines, polish, model downloads) checks certificates with the system trust store through `rustls-platform-verifier`, which needs the app's `Context`. Call `Native.init(context)` once per process before opening the first `Bolo`; it's idempotent. `Bolo(...)` throws `FfiException.Engine(CONFIG, ...)` until it has run, because a connection without the verifier panics, and a panic aborts a release build. UniFFI loads `libbolo_ffi.so` through JNA, so `Native` is the one class that talks plain JNI (`Java_dev_bolo_android_Native_initTls` in `crates/bolo-ffi/src/tls.rs`).

Release builds (M6) that turn on R8 need these keep rules: the verifier's Kotlin classes are only called from Rust, and JNA and UniFFI's bindings work by reflection.

```text
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
-keep class com.sun.jna.** { *; }
-keep class dev.bolo.ffi.** { *; }
```

## Models

Bolo keeps models in `noBackupFilesDir/models/<model-id>/`. The debug screen's Models section (ticket 05) shows the phone's tier and the recommended model, and downloads, cancels and deletes models. Downloads run in `ModelDownloadService`, a `dataSync` foreground service with a progress notification, through `bolo-models` (resume, SHA-256 check, atomic unpack). The phone's tier comes from `MemTotal`: 5000 MiB or more is high tier (Nemotron), less is low tier (Moonshine-tiny for English, Whisper-base Hinglish for Hinglish and Auto). To download from adb, which skips the mobile-data question:

```sh
adb shell am start -n dev.bolo.android/.MainActivity --es download moonshine-tiny
adb logcat -s bolo:*      # ... download moonshine-tiny: Downloaded in 9.6 s
```

To skip the network, push models from a desktop install instead:

```sh
adb push %LOCALAPPDATA%\Bolo\data\models\moonshine-tiny /data/local/tmp/models/
adb push %LOCALAPPDATA%\Bolo\data\models\silero-vad /data/local/tmp/models/
adb shell run-as dev.bolo.android mkdir -p no_backup/models
adb shell run-as dev.bolo.android cp -r /data/local/tmp/models/moonshine-tiny /data/local/tmp/models/silero-vad no_backup/models/
```

In Git Bash, set `MSYS_NO_PATHCONV=1`, or it rewrites the `/data/...` paths.

## Dictating without speaking (debug builds)

The debug screen replays a 16 kHz mono 16-bit WAV in real time through the same `Bolo.pushAudio` path the microphone uses. It logs the result under the `bolo` tag. `-S` restarts the app, so every run starts cold.

```sh
adb push testdata/clips/en/02-can-you-send-me.wav /data/local/tmp/en02.wav
adb logcat -c
adb shell am start -S -n dev.bolo.android/.MainActivity --es replay /data/local/tmp/en02.wav --es engine local:nemotron-stream
adb logcat -s bolo:*      # ... replay text [ENGLISH]: Can you send me the report by Friday.
```

## Cloud keys on the emulator (debug builds)

`--es import_keys <file>` stores the `provider=key` lines of a file in `filesDir` through `KeystoreSecretStore`, then deletes the file. Pipe the keys in through `run-as` so they never appear on a command line, in the shell history or in a file on the PC. A blank key (`groq=`) deletes it. `--ez polish true|false` turns Claude polish on or off.

```sh
# From whatever holds the keys (here a secret manager's CLI), never echoed:
get-key deepgram | sed 's/^/deepgram=/' | adb shell "run-as dev.bolo.android sh -c 'cat > files/keys.import'"
adb shell am start -S -n dev.bolo.android/.MainActivity --es import_keys keys.import
adb shell am start -n dev.bolo.android/.MainActivity --ez polish true --es replay /data/local/tmp/en02.wav --es engine deepgram-stream,local:moonshine-tiny
```

To check the offline fallback, turn the network off and back on around a replay: `adb shell svc wifi disable && adb shell svc data disable`, then `svc wifi enable` and `svc data enable`.

## Instrumented tests (emulator only)

`KeystoreSecretStoreTest` needs a real Keystore and `TlsTest` needs the network (it opens HTTPS connections to the providers and to badssl.com's bad certificates), so they run on a device; CI runs only the JVM tests. `connectedDebugAndroidTest` uninstalls the app afterwards, which deletes the models in `no_backup`. Either pass `-Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true` (and `ANDROID_SERIAL=<serial>` when more than one device is attached), or install and run the test APK by hand:

```sh
./gradlew assembleDebug assembleDebugAndroidTest
adb -s <serial> install -r app/build/outputs/apk/debug/app-debug.apk
adb -s <serial> install -r app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb -s <serial> shell am instrument -w dev.bolo.android.test/androidx.test.runner.AndroidJUnitRunner
adb -s <serial> uninstall dev.bolo.android.test
```

The test uses its own Keystore alias and preferences file, so the app's stored keys are never touched.
