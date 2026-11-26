# 05: Model download by RAM tier

**What to build:** the settings app can recommend, download, verify and delete models on the phone.

**Blocked by:** 01 (02 for the Hinglish entry)

**Status:** ready-for-agent

- [ ] FFI: `models()` (the manifest entries for this tier, with installed/size/licence), `recommend(total_ram_mb, language)` (`Manifest::recommend` with `Platform::Android`; Kotlin reads `ActivityManager.MemoryInfo.totalMem`), `download_model(id)`, `cancel_download(id)` and `delete_model(id)`. Progress arrives via `BoloListener::on_download(id, done, total)`, throttled to about 10 Hz.
- [ ] Downloads reuse `bolo-models::download` (resume via Range, sha256 verify, atomic unpack). Kotlin runs them inside a foreground service of type `dataSync` with a progress notification, so leaving the app doesn't kill a 600 MB download; the service stops when the download ends.
- [ ] Before starting: the free space check (unpacked size × 1.2), and a "this is X MB, you're on mobile data" confirm when not on an unmetered network.
- [ ] The tier threshold: phones report less RAM than they're sold with (the 6 GB emulator reports 5930 MiB of `MemTotal`; real 6 GB phones report about 5.5–5.8 GB). Set `tier_thresholds_mb.android-high` in the manifest so every 6 GB phone lands in the high tier (for example ≥ 5000 MiB reported), and test it with the emulator's value.
- [ ] The low tier hides Auto dual-pass: Auto there maps to the Hinglish model (spec). The engine config built for the phone sets `idle_unload_secs = 20`, `mic_warm_secs = 30` and `threads = min(4, big cores)`.
- [ ] Tests: recommendation by RAM/language (Rust), download progress throttling and cancel (against a local test server, as `bolo-models` tests do).
