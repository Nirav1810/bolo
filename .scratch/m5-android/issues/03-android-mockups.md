# 03: Android mockups

**What to build:** a design canvas for the Android UI. The user's approval gates tickets 06 and 07, as M3 ticket 04 gated the desktop UI.

**Blocked by:** none

**Status:** ready-for-human (mockup review)

- [x] Screens (phone frame, light and dark):
  - the keyboard panel idle, listening (waveform, live red), with partials, loading the model, with polish arriving, an error, a password field, and the "switch keyboard" affordance;
  - the onboarding steps: welcome, enable the keyboard, pick it, mic permission, language, model download (progress, Wi-Fi notice, low storage), and the test box;
  - settings: General, Models, Cloud keys, Dictionary & snippets, Apps, History and About.
- [x] Use the desktop design system (`apps/desktop/src/theme.css` tokens, `.scratch/ui-overhaul/spec.md`) adapted to Material 3: Roboto/system font, 48 dp touch targets, M3 navigation, and the dynamic-colour decision written down (recommend: our accent, not wallpaper colours, so the brand stays recognisable).
- [x] Panel height and layout chosen against Gboard's: the same height, so switching keyboards doesn't make the app jump.
- [x] Publish it as a Design artifact, then record the approved decisions and tokens in this ticket and in `apps/android/README.md`.

## Human: review

- [ ] The user reviews the canvas and approves or asks for changes.

## Decisions (proposed 2026-11-26, waiting for the review)

Mockups: the "Bolo Android UI" design canvas, https://claude.ai/artifact/2Zri3YLzaTiqDqpeYjCJfZ (private to the user). It has four pages: Overview and tokens, Keyboard panel (8 states), Onboarding (7 steps, step 6 in four variants) and Settings (home and 7 pages). Every screen is drawn in light and dark on a 360 × 800 dp phone at 1 px = 1 dp.

- **Panel = the pill, made touchable.** The keyboard panel's top strip is the desktop pill (dot, 16-bar waveform, "Listening…", "Polishing…", "Polished", "Inserted", errors with an icon). Under it sits one 88 dp mic, then a row of switch keyboard, space, backspace and the field's enter action. Tap the mic to start and stop, or hold it and let go. While listening, the mic becomes Stop and a 56 dp cancel (×) appears to its left. A settings button sits at the strip's right. A password or no-learning field replaces the mic with "Bolo doesn't listen in password fields. Nothing is recorded." and a "Switch to Gboard" button.
- **Panel height: 264 dp** above the system navigation bar, the same as Gboard's Normal height on a 360 dp-wide phone, so the app's field doesn't move when you switch. That's 8 top padding, a 48 strip, a 136 mic area, 48 keys plus an 8 gap, and 16 bottom padding. Ticket 06 measures Gboard on the emulator and adjusts by a few dp if it differs. There's no taller size in M5.
- **Dynamic colour: off.** Bolo uses its own indigo, not wallpaper colours, for three reasons. A wallpaper can make the primary red or pink, and red in Bolo means only "the mic is recording", so the two would clash. The keyboard stays recognisable next to Gboard, which does follow the wallpaper. And the phone matches the desktop and its screenshots. minSdk 28 also needs a fixed scheme for Android 9–11 either way. Light and dark follow the system setting.
- **Navigation: list-detail.** The launcher icon opens a home screen with a status card ("Ready to dictate", or the first thing to fix) and one grouped list of pages. Each page opens with a back arrow and a small top app bar. There's no bottom navigation bar: seven destinations is too many for one, and settings aren't tasks you switch between. The desktop's Engines page becomes two pages: the engine chain and models on **Models**, and the keys on **Cloud keys**. **General** shows the chain in one row that links to Models. Dictionary and snippets share one page.
- **Wording.** Labels, descriptions, empty states and errors are the desktop's, with "this PC" changed to "this phone". There's no push-to-talk key or autostart on Android. Hover actions (copy, delete) move into a ⋮ menu. Apps are keyed by package name (`com.whatsapp`). Built-in app rows in the mockup are examples until ticket 06 writes the Android rules.
- **Model download (onboarding step 6 and Models).** 6 GB or more: NVIDIA Nemotron 3.5 ASR Streaming 0.6B (475 MB, 682 MB on disk). 3–4 GB: Whisper-base Hinglish for Hinglish or Auto, Moonshine v2 Tiny English (30 MB, 44 MB on disk) for English, and Nemotron listed as "Needs a phone with 6 GB of RAM". Whisper-base Hinglish shows "about 80 MB" until ticket 02 publishes the real size. Off Wi-Fi, a dialog asks "Download on mobile data?" with "Wait for Wi-Fi" or "Download now". When space is short, an inline warning asks for the download plus the unpacked size (1.2 GB for Nemotron) and offers "Free up space". The download runs as a foreground service with a notification.
- **Type scale (sp), system font** (Roboto on most phones, Noto Sans Devanagari for देवनागरी). No text is smaller than 12 sp. Titles use weight 500, not M3's 400, to match the desktop's semibold.

  | M3 style | Size / line, weight | Used for | Desktop equivalent |
  | --- | --- | --- | --- |
  | headlineSmall | 24 / 32, 500 | Onboarding titles | `text-display` |
  | titleLarge | 22 / 28, 400 | Top app bar, dialog title | `text-title` |
  | titleMedium | 16 / 24, 500 | Card and model titles | `text-heading` |
  | bodyLarge | 16 / 24, 400 | List rows, fields, dictated text | — |
  | bodyMedium | 14 / 20, 400 | Descriptions, the pill | `text-sm` |
  | labelLarge | 14 / 20, 500 | Buttons, section titles (in primaryText) | — |
  | labelMedium | 12 / 16, 500 | Badges, step counter, captions | `text-xs` |

- **Colour roles** (Compose `ColorScheme`, plus a `BoloColors` CompositionLocal for the extras). Hex values come from `theme.css`'s oklch values. The desktop has moved from M3 ticket 04's `#2F5BEA` to T3's indigo, so Android follows `theme.css`.

  | Role | Light | Dark | Desktop token |
  | --- | --- | --- | --- |
  | primary (mic, filled buttons, switch on, progress) | `#1B4ED8` | `#2E64EA` | `--primary` |
  | onPrimary | `#FFFFFF` | `#FFFFFF` | `--primary-foreground` |
  | primaryText, extra (waveform, icons, links, section titles) | `#1B4ED8` | `#87AFFF` | `--primary-text` |
  | primaryContainer (selected segment, tonal button) | `#EAEFFB` | `#15213E` | `--primary-tint` |
  | background | `#FCFCFC` | `#000000` | `--background` |
  | surfaceContainerLowest (cards) | `#FFFFFF` | `#0E0E0E` | `--card` |
  | surfaceContainer (quiet fills, tiles) | `#F4F4F5` | `#1B1B1B` | `--muted` |
  | onSurface / onSurfaceVariant | `#18181B` / `#65656D` | `#F2F2F2` / `#A3A3A3` | `--foreground` / `--muted-foreground` |
  | outline / outlineVariant | `#8A8A94` / `#E4E4E7` | `#6B6B6B` / `#222222` | `--field-edge` / `--border` |
  | error | `#B91C1C` | `#F87171` | `--destructive-foreground` |
  | live, extra (only "the mic is recording") | `#E5484D` | `#FF5A5F` | `--live` |
  | success / warning, extra | `#047857` / `#A84D08` | `#34D399` / `#FBBF24` | `--success-` / `--warning-foreground` |
  | keyboard / key / function key, extra | `#EEEEF1` / `#FFFFFF` / `#DCDCE1` | `#121212` / `#2C2C2C` / `#1F1F1F` | new |

- **Shape and spacing (dp).** Keys 10, fields 8 (as desktop), cards 12 (as desktop), buttons, chips and the pill fully round, dialogs 28. Spacing is on a 4 dp grid with a 16 dp screen margin and 56 / 72 dp list rows. Every touch target is at least 48 dp: icon buttons are 48, buttons 40 tall inside a 48 target, keys 48, cancel 56 and the mic 88.
- **Icons:** Lucide, the desktop's set, as vector drawables at 24 dp with a 2 dp stroke.
- **Motion:** the desktop's ease-out curve `cubic-bezier(.23,1,.32,1)`: 150 ms press, 200 ms enter, 140 ms exit. The polish swap tints the new text for 600 ms. The mic's halo follows the input level. With "Remove animations" on, fades stay and movement goes.
- **Checked:** every artboard was rendered locally with Playwright (Edge) in both themes. Nothing is clipped or overflows the 360 × 800 screen. Every text and background pair used passes 4.5:1. The lowest is secondary text on the light keyboard panel (4.99:1) and on a light chat bubble (4.55:1).
