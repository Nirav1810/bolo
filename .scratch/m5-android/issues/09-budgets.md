# 09: Idle, battery and latency budgets

**What to build:** measure M5's budgets on the emulator, fix what misses, and write the real-phone checks.

**Blocked by:** 06

**Status:** ready-for-agent

- [ ] Hidden keyboard: `dumpsys meminfo dev.bolo.android` after the model's idle unload, and with the keyboard hidden: no `AudioRecord`, no engine threads (`/proc/<pid>/task` count), no wakelocks (`dumpsys power`), no jobs (`dumpsys jobscheduler`), no alarms. Script this as `scripts/android_idle.py` (adb).
- [ ] Shown, idle: the CPU is ~0% with the panel open and no dictation (`top -n` samples).
- [ ] Latency on the emulator: release → `on_final` p50/p95 for each tier's model (`Timings` logged by the IME into logcat, about 20 dictations from the `WavSource` test path, not a human), and the cold start after a process kill (model load). The ADR-0014 dual-pass Auto check: if Nemotron dual-pass misses the latency bar on the emulator by more than the single pass does, move `android-high` to single pass with a recorded number.
- [ ] Cross-compile `bolo` (the CLI) for Android and run `bolo run --speed 50` on the emulator via `adb shell` for WER/RTF on the 40 clips (Moonshine-tiny, Whisper-base Hinglish, Nemotron). Add a BENCH.md section, stating clearly that the numbers are emulator numbers.
- [ ] Budgets go in `docs/ARCHITECTURE.md` (the Android section), measured against the original spec's numbers.

## Human: real phones

- [ ] On a ≥ 6 GB phone: install the debug APK, run onboarding (Nemotron), and dictate English and Hinglish in WhatsApp, Gmail, Chrome and Notes. Note release → text by feel (instant / short wait / slow), and battery after a day with the keyboard installed (Settings › Battery: Bolo should be ≈ 0% when unused).
- [ ] On a 3–4 GB phone: the same with the recommended model (Moonshine-tiny for English, Whisper-base Hinglish for Hinglish).
