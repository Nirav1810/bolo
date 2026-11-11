# 04: Design the desktop UI (review gate)

**What to build:** Designed mockups for every M3 surface, published as an HTML artifact for the user to review, then the approved tokens committed to `apps/desktop`. No UI ticket (08, 09, 10) starts before the user approves.

**Blocked by:** none

**Status:** done (approved by the user 2026-11-11). The look was superseded by `.scratch/ui-overhaul/spec.md` (T3 Code style), 2026-11-11.

- [x] Use the `ui-ux-pro-max` and `emil-design-eng` skills. Light and dark, following Windows. Respect Windows 11 conventions (Mica-like surfaces, Segoe UI Variable or a chosen pairing, rounded corners).
- [x] Overlay pill states: listening (waveform + partial line), processing, polishing (pending swap), inserted, notice ("Copied, couldn't paste into an admin window"), error, cancelled. Include sizes and motion: show and hide timing, and waveform behaviour at silence and at speech.
- [x] Settings window: General (push-to-talk key, autostart, language mode, script), Engines and keys, Polish (on/off, tone, per-app list), Dictionary, History (search, copy, clear, retention), About.
- [x] Onboarding: welcome → mic check (including the Audio enhancements warning) → model (recommended, size, progress, cloud alternative) → try it.
- [x] Tray menu and icon states: idle, listening, paused.
- [x] Publish as one artifact and get the user's review. Record the decisions in this file.
- [x] Commit the tokens (colour, type, radius, spacing, motion) as the Tailwind theme in `apps/desktop`.

## Decisions (approved 2026-11-11)

Mockups: the "Bolo desktop UI" design canvas, https://claude.ai/artifact/EwvSRB7kq7D9YhEWKLhSzB (private to the user). Tokens: `apps/desktop/src/theme.css`.

- **Look.** Part of Windows 11, not on top of it: neutral Mica-like greys, the Windows type ramp in Segoe UI Variable (Inter only as an off-Windows preview fallback; Devanagari falls back to Nirmala UI), one blue accent for actions (`#2F5BEA` light, `#7C9BFF` dark). Light and dark follow the Windows setting through `prefers-color-scheme`.
- **Red means live.** The `live` colour appears only while the mic records: the pill dot and the tray icon. Notices and errors use warn/danger with an icon, never colour alone. Body text passes 4.5:1 in both themes.
- **Radius** 4 (controls), 8 (cards, windows, menus), 12 (dialogs), full (pill). **Spacing** on a 4 px grid; controls 32 px tall.
- **Motion.** Ease-out `cubic-bezier(.23,1,.32,1)` for enter/exit, ease-in-out `cubic-bezier(.77,0,.175,1)` for the pill's width; no ease-in. 120 ms press/hover, 180 ms enter and crossfade (2 px blur), 140 ms exit, 220 ms resize. Menus and the settings window don't animate (Windows does). Reduced motion keeps fades, drops movement, freezes the waveform.
- **Overlay pill** (ticket 08). 40 px tall, fits content up to 520 px, bottom-centre 24 px above the work area. Shows at Start with y 8→0 and scale .96→1. States: listening (dot + waveform, "Listening…" until the first partial), transcribing (only if text takes over 250 ms after release), inserted, polishing (shimmer) → polished, notice ("Copied. Couldn't paste into an admin window" + Ctrl+V), error, cancelled. Hides 600 ms after insert, or after the swap/skip when one is pending; notices and errors stay 4 s. The partial is one line; older words fade off the left edge.
- **Waveform.** 16 bars, 3 px wide, 2 px gap. Silence (below −50 dBFS): every bar rests at 3 px and nothing moves. Speech: `on_level` mapped from −50…−10 dBFS onto 3–24 px, pushed in from the right one bar per display frame; bars rise in one frame and fall over 120 ms.
- **Settings window** (ticket 09). Windows 11 Settings layout: left nav (General, Engines and keys, Polish, Dictionary, History, About) and setting cards. General: push-to-talk key, microphone + Test, Start with Windows (off), language mode (Auto/English/Hinglish segmented), Hinglish script (Roman/Devanagari). Engines: local model card in use + cloud engines; keys say they live in Credential Manager. Polish: on/off, tone (Neutral/Casual/Formal), per-app list with "Default"/"Your choice" tags. History: search, Copy per row, swap outcome in the row's caption, Clear history…, save on/off, delete after 30 days.
- **Onboarding** (ticket 10). Welcome (hold Caps Lock, speak, let go) → mic check with a level meter and the Audio enhancements warning ("Your mic starts N ms late", Open sound settings, Check again, Continue anyway) → model (recommended card with progress, time left, "resumes if you close this window"; a cloud engine with a key as the alternative; Next disabled until done) → try it (practice box, success line with the measured latency). Four-dot stepper in the footer.
- **Tray.** A monochrome mic matching the taskbar; red while listening; a slash at 70% opacity when paused. Left-click opens Settings; right-click opens the menu: header "Bolo · Ready", Pause dictation, History, Settings, Language ›, Polish ✓, Quit Bolo. Tooltips "Bolo: ready/listening/paused". ICO frames 16, 20, 24, 32 px.
