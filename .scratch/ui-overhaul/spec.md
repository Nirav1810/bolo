# UI overhaul: T3 Code's design language

**Status:** ready-for-human (user review of the new look in the running app)

The user liked how Bolo works but not how it looked. They asked for every window to be redone
in the style of T3 Code (github.com/pingdotgg/t3code, `apps/web/src`), and for each page to be
rated against it. This replaces the Windows 11 look from M3 ticket 04. Behaviour, Rust, and the
pill's state machine and timings are unchanged.

## Decisions

- **Tokens** (`apps/desktop/src/theme.css`). These are T3 Code's semantic names: `background`, `card`,
  `muted-foreground`, `border`, `input`, `primary`, `accent`, `sidebar-*` and so on.
  - Light mode uses zinc neutrals on a `zinc-25` background with white cards.
  - Dark mode uses a pure black background and `#0d0d0d` cards, with borders at 9% white.
  - Primary is T3's indigo, `oklch(0.488 0.217 264)` in light and `oklch(0.571 0.21 264)` in dark.
  - `primary-text` is a lighter indigo for thin marks on dark surfaces, such as icons, the
    waveform and badges.
  - `live` red still means only "the mic is recording". The Nightly violet is kept.
- **Type.** Inter Variable is bundled (`@fontsource-variable/inter`, as T3 does), with Segoe UI behind
  it and Nirmala UI for Devanagari. Sizes are 14 px body, 12 px descriptions, 11 px metadata,
  15 px section titles and 20 px page titles.
- **Shape.** Radius is 8 px for controls and 12 px for cards. Controls are 32 px tall (28 px small).
  Buttons scale to 0.97 when pressed, and the primary button has an inset highlight.
- **Icons.** Lucide (`@lucide/svelte`, pinned 1.60.0), as in T3.
- **Components** (`src/lib/ui/`): Button (with primary, outline, ghost, ghost-muted,
  ghost-destructive and destructive-outline variants), Switch, Segmented (a radiogroup that
  arrow keys move through), Badge, Kbd, Alert, Empty, SettingsSection and SettingsRow (T3's
  grouped rows with hairline dividers), and ConfirmButton (destructive actions ask once).
  Native inputs and selects stay CSS classes (`.input`, `.select`, `.textarea` in `ui.css`).
- **Settings shell.**
  - The sidebar shows the app icon, the name and a build badge. The nav uses icons and
    titled groups (Dictation, Activity, App), and Up and Down move through it.
  - The footer shows the version, or an "Update available" card that links to About.
  - The content is one centred 700 px column with the page title and blurb, and pages fade in.
- **History.** Dictations are grouped by day. Each row has its time, text, the pre-polish text
  struck through, and badges for app, language and polish outcome. Copy, Copy original and
  Delete appear on hover or focus. Empty and no-match states have their own messages.
- **Onboarding.** A header shows a step counter and a four-segment progress bar. Each step has an
  icon tile, a 24 px title and a card. Welcome explains Hold, Speak and Let go in three steps,
  and the mic check has a 32-bar level meter. The footer has Back, and Next with an arrow.
- **Pill.** The pill uses the new surface tokens and Inter at 13 px. Waveform bars are drawn in
  `primary-text`, and their opacity follows the level, as in T3's VoiceWaveform. The geometry,
  the states and the 16-bar envelope from ticket 08 are unchanged.
- **Web Interface Guidelines pass** (vercel-labs, fetched 2026-11-11), with these fixes:
  - destructive icon actions now confirm;
  - placeholders end in "…";
  - inputs have `autocomplete`/`spellcheck` set;
  - day headings sit in heading order;
  - long dictations wrap;
  - the switch thumb animates with transform only;
  - `overscroll-behavior` and `touch-action` are set.
  - Deliberately not followed: Title Case. Bolo and T3 both use sentence case.
- **Fixed on the way.** The Polish switch's `id="polish"` collided with the `#polish` tab hash
  that Rust opens, which scrolled the page to the switch.

## Ratings against T3 Code (out of 10)

| Surface | Before | After | What keeps it from a 10 |
| --- | --- | --- | --- |
| Settings shell and nav | 4 | 8.5 | No settings search or command palette; native Windows title bar |
| General | 5 | 8.5 | The shortcut picker is a button, not a recorder with key chips |
| Engines and keys | 4 | 8 | Still a dense page; chain order isn't drag-and-drop |
| Polish | 4 | 8.5 | The per-app list has no search |
| Dictionary | 4 | 8 | No search or bulk select over long lists |
| History | 4 | 8.5 | No virtualisation over hundreds of rows; no toast on copy |
| About | 5 | 8.5 | — |
| Onboarding | 5 | 8.5 | No illustration of the pill in action |
| Overlay pill | 6 | 8 | No backdrop blur (the window is transparent over other apps) |
| Debug | 3 | 7 | Developer-only, so only lightly restyled |

## Next, if wanted

Settings search (T3's sidebar search), a custom title bar (`decorations: false` + drag region),
toasts for copy/save, and drag-to-reorder for the engine chain.
