# 03: Android mockups

**What to build:** a design canvas for the Android UI. The user's approval gates tickets 06 and 07, as M3 ticket 04 gated the desktop UI.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Screens (phone frame, light and dark):
  - the keyboard panel idle, listening (waveform, live red), with partials, loading the model, with polish arriving, an error, a password field, and the "switch keyboard" affordance;
  - the onboarding steps: welcome, enable the keyboard, pick it, mic permission, language, model download (progress, Wi-Fi notice, low storage), and the test box;
  - settings: General, Models, Cloud keys, Dictionary & snippets, Apps, History and About.
- [ ] Use the desktop design system (`apps/desktop/src/theme.css` tokens, `.scratch/ui-overhaul/spec.md`) adapted to Material 3: Roboto/system font, 48 dp touch targets, M3 navigation, and the dynamic-colour decision written down (recommend: our accent, not wallpaper colours, so the brand stays recognisable).
- [ ] Panel height and layout chosen against Gboard's: the same height, so switching keyboards doesn't make the app jump.
- [ ] Publish it as a Design artifact, then record the approved decisions and tokens in this ticket and in `apps/android/README.md`.

## Human: review

- [ ] The user reviews the canvas and approves or asks for changes.
