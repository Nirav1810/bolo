# 10: First-run onboarding and model download

**What to build:** The onboarding flow from the approved design: welcome → mic check → model → try it.

**Blocked by:** 01, 02, 04

**Status:** ready-for-human (built; a first run on a clean profile is owed)

- [x] It opens on first run (no settings file) and from the tray afterwards.
- [x] Mic check: open the default device and measure leading digital silence like `mic-test`. Over 200 ms → explain how to turn off Audio enhancements, with a button that opens the Windows sound settings, and a re-test.
- [x] Model: `Manifest::recommend(Platform::Windows, total RAM, hinglish)` with its size. Download through `bolo_models::Downloader` with progress, cancel and resume, verified by SHA-256. Alternatively, pick a cloud engine and paste its key.
- [x] Try it: hold the push-to-talk key and dictate into a text box in the onboarding window. Shows the timings.
- [x] Completing onboarding writes settings. Quitting partway resumes at the same step next time.

## Comments

**2026-11-11 (agent).** `src/onboarding/Onboarding.svelte` with four steps and a stepper. It opens when there was no settings file (`Loaded::fresh`), and from the tray's "Set up Bolo…".
- Progress: `Settings::onboarding_step` holds the step and is `None` once finished, so a file from an older build never shows onboarding.
- Mic check (`mic.rs`): a cold open of the default mic for 3 s, with levels on `bolo://mic`. Leading exact zeros over 200 ms show the Audio enhancements warning, with Open sound settings (`ms-settings:sound`) and Check again. Hearing nothing shows its own warning. A live run here read 0 ms.
- Model: the recommended model (`Manifest::recommend(Desktop, RAM, Hinglish unless English)`) with progress, time left and Pause. A paused download resumes from its `.part` file and is SHA-256 verified. Once done, the model becomes primary. The alternative is a Deepgram key, tested, saved and set as `deepgram-stream`. Next stays disabled until one of the two is ready.
- Try it: a practice box in the window. The push-to-talk key pastes into it; the success line shows the engine's release → text time from `on_final`.

**Manual check owed (human):** rename `%APPDATA%\Bolo\settings.toml` away (or use a clean profile), start Bolo, and walk all four steps. Quit at step 3 and restart: it should reopen at step 3.
