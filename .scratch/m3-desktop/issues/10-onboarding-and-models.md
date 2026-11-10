# 10: First-run onboarding and model download

**What to build:** The onboarding flow from the approved design: welcome → mic check → model → try it.

**Blocked by:** 01, 02, 04

**Status:** ready-for-agent

- [ ] It opens on first run (no settings file) and from the tray afterwards.
- [ ] Mic check: open the default device and measure leading digital silence like `mic-test`. Over 200 ms → explain how to turn off Audio enhancements, with a button that opens the Windows sound settings, and a re-test.
- [ ] Model: `Manifest::recommend(Platform::Windows, total RAM, hinglish)` with its size. Download through `bolo_models::Downloader` with progress, cancel and resume, verified by SHA-256. Alternatively, pick a cloud engine and paste its key.
- [ ] Try it: hold the push-to-talk key and dictate into a text box in the onboarding window. Shows the timings.
- [ ] Completing onboarding writes settings. Quitting partway resumes at the same step next time.
