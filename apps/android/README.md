# apps/android (M5)

A Kotlin + Jetpack Compose voice keyboard (`InputMethodService`, ticket 06) that loads `libbolo_ffi.so` through UniFFI (ADR-0023). It targets minSdk 28 and ships arm64-v8a, plus x86_64 in debug builds for the emulator. The plan is in `.scratch/m5-android/`.

Today (ticket 01) the app is a debug screen. Hold the button to dictate with the engine, or replay a WAV file.

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

## Models

There's no model download yet (ticket 05). Push models from a desktop install. Bolo looks for them in `noBackupFilesDir/models/<model-id>/`.

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
