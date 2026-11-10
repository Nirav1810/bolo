# 04: Design the desktop UI (review gate)

**What to build:** Designed mockups for every M3 surface, published as an HTML artifact for the user to review, then the approved tokens committed to `apps/desktop`. No UI ticket (08, 09, 10) starts before the user approves.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Use the `ui-ux-pro-max` and `emil-design-eng` skills. Light and dark, following Windows. Respect Windows 11 conventions (Mica-like surfaces, Segoe UI Variable or a chosen pairing, rounded corners).
- [ ] Overlay pill states: listening (waveform + partial line), processing, polishing (pending swap), inserted, notice ("Copied, couldn't paste into an admin window"), error, cancelled. Include sizes and motion: show and hide timing, and waveform behaviour at silence and at speech.
- [ ] Settings window: General (push-to-talk key, autostart, language mode, script), Engines and keys, Polish (on/off, tone, per-app list), Dictionary, History (search, copy, clear, retention), About.
- [ ] Onboarding: welcome → mic check (including the Audio enhancements warning) → model (recommended, size, progress, cloud alternative) → try it.
- [ ] Tray menu and icon states: idle, listening, paused.
- [ ] Publish as one artifact and get the user's review. Record the decisions in this file.
- [ ] Commit the tokens (colour, type, radius, spacing, motion) as the Tailwind theme in `apps/desktop`.
