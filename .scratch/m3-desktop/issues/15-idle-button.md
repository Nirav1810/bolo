# 15: Always-on idle button and hands-free dictation

**What to build:** The overlay pill stays on screen when Bolo is idle, as a small bar that grows on hover, the way Wispr Flow's does. Clicking it starts a hands-free dictation (no key held) with Stop and Cancel buttons. This amends the ticket 04 design, where the pill was hidden when idle.

**Blocked by:** 08

**Status:** ready-for-human (the mouse checks below)

## Decisions (the user, 2026-11-11)

- **Idle look.** A small rounded bar (40×8 px) bottom-centre. On hover it grows into a pill: "Click or hold Caps Lock to dictate".
- **Click.** Starts a hands-free dictation into the app in front (the click never takes focus). The listening pill then shows Cancel (✕) and Stop (✓). Stop, or a Caps Lock press, finishes it; ✕ or Esc cancels.
- **Hides** in fullscreen apps on that monitor and while Bolo is paused.
- **Setting.** "Show the button on screen", on by default. It's saved in `settings.toml` (`[overlay] idle_button`), and the tray has a checkbox for it until the settings window (ticket 09) does.
- **Draggable** along the bottom; the spot is remembered (`[overlay] position`, 0–1 across the work area).

## How

- The window stays click-through and non-activating: WebView2 takes focus when clicked, and that would break pasting. A `WH_MOUSE_LL` hook, installed only while the pill shows, hit-tests the pill's regions (which the webview reports), swallows presses on them, and turns them into hover, click and drag.
- The pill state machine gains an Idle view (shown instead of hiding while the button is allowed) and a hands-free flag on Listening.

## Acceptance

- [ ] Idle bar shows at startup bottom-centre on the monitor in front, follows the window in front to another monitor, and hides in fullscreen and when paused.
- [ ] Hover grows it; moving away shrinks it. A click starts a dictation into the app that was in front, which keeps focus and its caret.
- [ ] Hands-free: ✓ (or a Caps Lock press) inserts the text, ✕ (or Esc) cancels.
- [ ] Dragging moves it along the bottom and the spot survives a restart.
- [ ] The tray checkbox hides and shows it and is saved.
- [x] Unit tests for the gesture machine, the pill's idle rules and placement with a position.

## Notes (2026-11-11)

- `pointer.rs`: the pure `Gesture` (hover, click, drag with a 4 px slop; buttons win over the pill) and the `WH_MOUSE_LL` hook thread, hooked only while the pill shows. `overlay.rs` now owns placement (`place(work, scale, position)`, `position_of`), polls the monitor in front every 500 ms (fullscreen, a new monitor), maps the webview's regions onto the screen and handles clicks and drags. `pill.rs` gains `View::Idle { hover, key }`, `Listening { hands_free }`, and `Input::Idle/Hover/HandsFree`.
- Found live: the idle view went out at startup before the pill page loaded; the page now asks for it (`pill_view`).
- Checked live: the idle bar shows bottom-centre at startup. Hover, click and drag need the user's mouse.
