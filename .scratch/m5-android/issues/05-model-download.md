# 05: Model download by RAM tier

**What to build:** the settings app can recommend, download, verify and delete models on the phone.

**Blocked by:** 01 (02 for the Hinglish entry)

**Status:** done

- [x] FFI: `models()` (the manifest entries for this tier, with installed/size/licence), `recommend(total_ram_mb, language)` (`Manifest::recommend` with `Platform::Android`; Kotlin reads `ActivityManager.MemoryInfo.totalMem`), `download_model(id)`, `cancel_download(id)` and `delete_model(id)`. Progress arrives via `BoloListener::on_download(id, done, total)`, throttled to about 10 Hz.
- [x] Downloads reuse `bolo-models::download` (resume via Range, sha256 verify, atomic unpack). Kotlin runs them inside a foreground service of type `dataSync` with a progress notification, so leaving the app doesn't kill a 600 MB download; the service stops when the download ends.
- [x] Before starting: the free space check (unpacked size × 1.2), and a "this is X MB, you're on mobile data" confirm when not on an unmetered network.
- [x] The tier threshold: phones report less RAM than they're sold with (the 6 GB emulator reports 5930 MiB of `MemTotal`; real 6 GB phones report about 5.5–5.8 GB). Set `tier_thresholds_mb.android-high` in the manifest so every 6 GB phone lands in the high tier (for example ≥ 5000 MiB reported), and test it with the emulator's value.
- [x] The low tier hides Auto dual-pass: Auto there maps to the Hinglish model (spec). The engine config built for the phone sets `idle_unload_secs = 20`, `mic_warm_secs = 30` and `threads = min(4, big cores)`.
- [x] Tests: recommendation by RAM/language (Rust), download progress throttling and cancel (against a local test server, as `bolo-models` tests do).

## Outcome

- **FFI** (`crates/bolo-ffi/src/models.rs`, a second `#[uniffi::export] impl Bolo`):
  - The calls: `models()`, `recommend(total_ram_mb, language)`, `tier(total_ram_mb) -> DeviceTier { High, Low }` (so ticket 07 can hide dual-pass Auto on the low tier), `download_model(id) -> bool`, `cancel_download(id) -> bool` and `delete_model(id)`.
  - `ModelInfo` carries the id, name, spec, languages, `hinglish`, download and disk bytes, `space_needed`, `installed`, `recommended`, `downloading`, and the licence's id, URL and attribution.
  - **Deviation:** progress is `on_download(ModelDownload { id, phase, downloaded, total, error })`, not `(id, done, total)`. A UI needs the phase (`Downloading`, `Verifying`, `Unpacking`, `Done`, `Failed`, `Cancelled`) and the error.
  - `download_model` **blocks** its caller, the service's worker thread. It runs on its own current-thread tokio runtime with a new HTTP client for each download. It returns `true` when the model is installed and `false` when cancelled. Errors are `FfiError::Download { detail }` and `FfiError::NoSpace { needed, available }`.
  - Downloads are registered process-wide, keyed by models directory and id. A cancel from any `Bolo` stops the download. Every `Bolo` on the same models directory hears its events, so the service's `Bolo` and the settings screen's stay in step. After an install or a delete, every engine on those models gets `update_config`. Deleting a model is refused while it downloads.
- **Shared code went into `bolo-models`, not the FFI.**
  - `bolo_models::catalog` now holds the desktop's model list: `ModelView` (which gained the licence id and URL), `views(manifest, tier, root, recommended)`, `VAD_MODEL`, and `ModelEntry::space_needed` and `speaks_hindi`.
  - `Downloader::install_with_vad` installs the model and then the VAD, and sends `Done` once both are in.
  - Two new free functions: `partial_bytes(root, entry)`, and `uninstall(root, entry)`, which deletes the model and its `.part`. The unused async `Downloader::remove` is gone.
  - The desktop's `models.rs` now calls these. Its behaviour and tests are unchanged.
- **Throttle.** `bolo-models` reports every MiB. `models::Throttle` lets a `Downloading` event through at most every 100 ms (`PROGRESS_INTERVAL`) and always passes a phase change and the last byte. The notification updates at most every 500 ms, because Android drops faster updates.
- **Free space.** `space_needed` is the rest of the archive, plus the unpacked size and 20%, plus the same for the VAD if it isn't installed.
  - The ticket asked for unpacked × 1.2. I added the archive because it stays on disk next to the unpacked files until the install finishes: moonshine-tiny needs 83 MB, not 53 MB.
  - Kotlin compares it with `StorageManager.getAllocatableBytes` before starting.
  - Rust checks again with `statvfs` (a new `libc` dependency, unix only) and refuses with `NoSpace`.
- **Metered network.** When `ConnectivityManager.isActiveNetworkMetered` is true, a dialog asks "Download on mobile data? *Name* is N MB, and you're on mobile data." with Download / Not now. Checked on the emulator with Wi-Fi off; cellular counts as metered there.
- **Foreground service.** `ModelDownloadService` is a `dataSync` foreground service.
  - It runs one download at a time on a worker thread, through its own `Bolo`, with a progress notification and a Cancel action.
  - It stops when its queue is empty and leaves a final "Downloaded" or "Download failed: …" notification. On API 35+, `onTimeout` cancels the download.
  - Permissions: `INTERNET`, `ACCESS_NETWORK_STATE`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC` and `POST_NOTIFICATIONS`. On API 33+, the first Download tap asks for `POST_NOTIFICATIONS`. The download runs whether or not it's granted.
  - It also catches UniFFI's `InternalException`, which is how a Rust panic reaches Kotlin when panics unwind.
- **Tier threshold.** `tier_thresholds_mb.android-high` is now **5000** in the manifest, and the code default (`ANDROID_HIGH_MB`) matches.
  - Tested with reported values: the emulator's 6 072 060 kB (5929 MiB), 5632, 5939 and 7680 MiB are high tier. 3700, 3850 and 2800 MiB are low tier.
  - The engine reads the tier from `MemTotal` in `/proc/meminfo`, the same value `ActivityManager.MemoryInfo.totalMem` reports. Apps can read it; on the emulator it gave 5929 MiB.
  - `recommend()` and `tier()` take the value Kotlin reads, as the ticket asks.
  - When the RAM can't be read, the phone counts as high tier and the user's engine choice stands.
- **Low tier and Auto.** `engine_config` runs `models::fit_engines`.
  - On a low-tier phone, a local model the tier can't run (Nemotron, or a desktop model) is swapped for the tier's model for the language: Whisper-base Hinglish for Auto and Hinglish, Moonshine-tiny for English.
  - So the default `local:nemotron-stream` with Auto runs the Hinglish model. The settings file keeps the user's choice.
  - A manifest test checks that no low-tier model has a per-stream language option, so Auto on the low tier is never dual-pass.
- **Budgets.** `idle_unload_secs = 20` and `mic_warm_secs = 30` (set in ticket 04), and `threads = min(4, big cores)`.
  - Big cores are the cores whose `cpuinfo_max_freq` is higher than the slowest cluster's, or every core when they're all the same. The emulator reads 4.
  - Without cpufreq, it falls back to `available_parallelism`.
- **Debug UI.** The debug screen gained a `ModelsPanel`. It shows "5929 MiB: HIGH tier. For AUTO: nemotron-stream." and lists the tier's models with Download / Cancel / Delete and a progress bar.
  - `--es download <id>` starts the service from adb.
  - `BoloFiles` holds the paths every `Bolo` uses.
  - The real Models page is ticket 07.
- **Tests.**
  - `bolo-models`: the tier for the reported-RAM cases, the threshold default, no dual-pass on low-tier models, each tier's list, the recommendation and its licence, and `space_needed`.
  - `bolo-ffi` unit tests: the throttle (one event per 100 ms, phase changes, the last byte, 20 events from 2 s at 200 MiB/s), `fit_engines`, `/proc/meminfo` parsing, big cores, threads and tier.
  - `bolo-ffi/tests/models.rs`, against a local HTTP/1.1 server with Range support:
    - An install verifies, with at most one progress event per 100 ms elapsed (plus 2).
    - A cancel at 2 MiB keeps the `.part`, and the next download resumes with `Range: bytes=N-`.
    - A cancel before the start downloads nothing.
    - A bad checksum fails and drops the bytes.
    - The VAD comes along, and `Done` is sent once.
  - The same file, through `Bolo`:
    - Tier and recommendation: the 6 GB emulator gets Nemotron, and 3700 MiB with Auto gets Whisper-base Hinglish.
    - The list's order follows the language.
    - The engine config on the low tier: the Hinglish model, 20/30 s, 2 threads, and never more than 4.
    - Events reach every `Bolo` on the models.
    - Delete removes the partial file too.
    - The errors, and the `NoSpace` message.
- **On the emulator** (bolo-api36, debug build, x86_64):
  - I deleted `moonshine-tiny` from `no_backup/models` and downloaded it through the app (Download, then the notification permission prompt, then Allow) from the manifest's GitHub release URL.
  - **The download took 98.7 s:**
    - About 78 s to fetch the 29.9 MB archive over the emulator's network (~0.4 MB/s).
    - 1.2 s to check the SHA-256.
    - 19 s to unpack: an unoptimized debug build decompressing bzip2.
  - A second run, on cellular after the metered-network dialog, took 80.1 s.
  - Afterwards `.bolo-installed` held the manifest's sha256 (`9ec31b34…7164f8d`) and `.downloads/` was empty.
  - A debug replay of `en/02` then dictated with the downloaded model: "And you send me the report by Friday." Model load took 6.7 s, release → text 941 ms.
  - The other session on the same AVD rebooted it mid-download, which left a 1.9 MB `.part` on disk. The resume itself is covered by the Rust test.
- **TLS (ticket 08).** reqwest's `rustls-platform-verifier` needs its Android init, or the first HTTPS handshake panics. The emulator check above ran with ticket 08's work-in-progress `Native.initTls` overlaid locally. Ticket 08 merged first (#82), so the rebase of this change added `Native.init(this)` at the start of `ModelDownloadService.open()`, before its `Bolo` opens. The download was then re-checked on the emulator with the merged code.
