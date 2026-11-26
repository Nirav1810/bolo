# M5: Android voice keyboard

**Why:** the original plan's M5 is the Android voice keyboard. That means `bolo-ffi` (UniFFI → Kotlin), the IME, a settings app and a model download by RAM tier. It has to work in WhatsApp, Gmail, Chrome and Notes, on a phone with ≥ 6 GB and on one with 3–4 GB (Moonshine or Whisper-base), within the idle budgets, and with no battery use while the keyboard is hidden (docs/M4-REPORT.md, "What M5 starts with"). M6 is the Android release pipeline (signing, Play Store, F-Droid), so M5 ships no store build.

The user started M5 on 2026-11-26. M4's sign-off is implied by that, and the M3/M4 manual checklists carry over.

## Decisions from planning (2026-11-26)

- **Keyboard shape: voice-only IME** (ADR-0022). The keyboard panel has a big mic button plus space, backspace, enter and a button that switches back to the previous keyboard. There's no QWERTY layout and no floating bubble or accessibility service.
- **Test devices: emulator for now.** Development and the M5 acceptance run use the Android emulator. Debug builds include `x86_64` next to `arm64-v8a` for it. The real-phone checks (≥ 6 GB and 3–4 GB) are written as Human items and owed by the user, like the M3/M4 checklists.
- **Low-tier Hinglish: do the export in M5.** `whisper-base-hinglish` gets converted to sherpa's Whisper layout, int8-quantized, hosted on a GitHub release in this repo and switched from `needs-export` to `available`.
- **UI: mockups first.** A design canvas covers the keyboard panel, onboarding, model download and settings, in the desktop's design system adapted to Material 3 conventions. The user approves it before any UI ticket starts (as M3 ticket 04 did).

## User-facing behaviour

- **Install and onboarding.** The user installs a debug APK (sideload; M6 does the stores). Onboarding goes: enable Bolo in the system keyboard list → pick it → grant the mic → choose a language (Auto, English or Hinglish) → download the recommended model. The phone's RAM decides the recommendation: Nemotron on ≥ 6 GB; on 3–4 GB, Moonshine-tiny for English or Whisper-base Hinglish for Hinglish/Auto. A test dictation box closes onboarding. Low-tier phones don't get Auto as dual-pass (ADR-0014): there, Auto means the Hinglish model.
- **Dictating.** In any text field, switch to Bolo. Tap the mic to start and tap again to stop, or hold to talk and release to stop. Words appear as underlined composing text while you speak, then commit as the final Layer 1 text. With polish on, the polished text replaces that dictation in place when it arrives, unless you've typed or moved the cursor since. Cancel (×) throws the dictation away. Space, backspace (with repeat), enter (the field's action: send, search, go) and "switch keyboard" are on the panel.
- **Per-app behaviour.** The settings Apps list keys on the package name (`com.whatsapp`, `com.google.android.gm`, …), with the same Polish/Tone/Format fields as desktop (ADR-0020). Bundled Android rules give sensible defaults. Password and other sensitive fields show a notice instead of the mic, and nothing is recorded.
- **Settings app** (the launcher icon). It has General (language, engine, model, polish), Models (download, delete, storage), Cloud keys (Deepgram, Groq, Sarvam, Anthropic), Dictionary & snippets, Apps, History (text only, 30 days) and About. The settings file uses the same schema as desktop (`settings.toml` v2), stored in the app's private files dir.
- **Cloud engines.** Deepgram, Groq and Sarvam engines, and Claude polish, work as on desktop when a key is set. Keys are encrypted with the Android Keystore and never leave the device except to the provider.
- **Budgets.** While the keyboard is hidden: the mic is closed, no engine thread runs, no wakelock is held and no job is scheduled. The model unloads 20 s after the last dictation, and immediately on `onTrimMemory`. A shown keyboard warms the mic only after the first mic tap, never just because it opened.

## Architecture in one paragraph

`bolo-ffi` exposes one UniFFI object, `Bolo`, which owns an `Engine`, the `bolo-store` settings/history and the model manifest. Kotlin calls it and gets callbacks through a `BoloListener` foreign trait. Audio comes from Kotlin's `AudioRecord` (`VOICE_RECOGNITION` source) and is pushed into the engine through the existing `AudioSource` seam (ADR-0023). Keys live in a Keystore-backed foreign `CredentialStore`, the TLS verifier is `rustls-platform-verifier`'s Android component, and paths come from Kotlin (`filesDir`, `noBackupFilesDir/models`). The engine runs inside the IME's process: it's created when the keyboard first needs it and released on `onTrimMemory` or after the idle timeout.

## Out of scope

- Signing, a release keystore, Play Store, F-Droid and the APK in GitHub releases (M6). CI builds a debug APK as an artifact only.
- A typing (QWERTY) keyboard, a floating bubble and accessibility services.
- iOS.
- A local LLM for polish.
- Auto-update on Android (stores handle it in M6).
- Opus/FLAC uploads on mobile data (the DECISIONS.md note). They stay a follow-up unless ticket 09's numbers show cloud uploads are slow on mobile.

## Tickets

1. `01-android-tracer`: the Gradle project, `bolo-ffi` minimal surface, cargo-ndk build, Kotlin bindings, and a debug activity that dictates with a local model on the emulator. Also the CI job.
2. `02-whisper-hinglish-export`: export, quantize and host `whisper-base-hinglish`, then measure it on the 40-clip set.
3. `03-android-mockups`: the design canvas for the keyboard, onboarding, model download and settings. The user's approval gates 06 and 07.
4. `04-ffi-store`: settings, history, secrets (Keystore) and paths on Android through `bolo-ffi`, with `bolo-store` building for Android.
5. `05-model-download`: the manifest and RAM-tier recommendation over FFI, with download progress, resume, verification and delete.
6. `06-ime`: the `InputMethodService` and keyboard panel, composing-text insertion, polish replace, per-app profiles and sensitive fields (ADR-0022).
7. `07-settings-onboarding`: the launcher app's onboarding and settings pages.
8. `08-cloud-on-android`: TLS verifier init, the cloud engines, Claude polish and the usage meter on Android.
9. `09-budgets`: idle RAM, battery (no wakelocks or jobs while hidden), and latency and WER on the emulator, plus a BENCH section and the Human real-phone checks.
10. `10-m5-report-and-v0.6.0`: the emulator acceptance run in WhatsApp-like, Gmail, Chrome and Notes fields, the M5 report and `v0.6.0`. This is the milestone gate.

Blockers: 01 and 02 have none and can start now, and 03 needs only the user's review. 04 and 05 need 01. 06 needs 01, 03 and 04. 07 needs 03, 04 and 05. 08 needs 04. 09 needs 06. 10 needs every other ticket.

## Risks

- **Emulator realism.** The emulator runs x86_64 on this machine, so latency and RAM numbers won't match a phone. The report states that, and the Human checks on real phones set the acceptance numbers.
- **sherpa-onnx on Android.** `sherpa-onnx-sys` 2.0.1 downloads the Android shared libraries (`jniLibs/<abi>`). A probe on 2026-11-26 found that `bolo-ffi` and `bolo-store` (bundled SQLite, keyring-core, reqwest/ring) build and link for `aarch64-linux-android` with `cargo ndk -P 28`. The only change needed was pointing `SHERPA_ONNX_LIB_DIR` at the Android `jniLibs/arm64-v8a`. Ticket 01 confirms they load from the APK, and that `libonnxruntime.so` and `libsherpa-onnx-c-api.so` get packaged.
- **Nemotron on a 6 GB phone.** The model is 1.4 GB of RAM. Dual-pass Auto may be too slow on a mid-range ARM core. Ticket 09 measures it, and a slow result moves the `android-high` Auto default to a single pass.
- **IME process memory.** The engine lives in the keyboard's process, which Android kills freely. A cold start after a kill reloads the model (~2–3 s on a phone). The keyboard shows "Loading model…" instead of dropping the dictation.
