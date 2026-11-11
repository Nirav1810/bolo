# 08: Overlay pill

**What to build:** The overlay pill from the approved design (ticket 04): a transparent, always-on-top, non-activating, click-through window that shows the dictation's state, a live waveform and the partial text.

**Blocked by:** 01, 04

**Status:** ready-for-human (the speech check below)

- [x] The window never takes focus or receives clicks (`WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW`). Not in the taskbar or Alt+Tab. Verified by dictating into Notepad with the caret still blinking.
- [x] Placed bottom-centre on the monitor of the window in front, DPI-aware.
- [x] Waveform from `on_level`, coalesced to one update per frame. Partials on one line, truncated from the left.
- [x] States from the design: listening, processing, polishing, inserted, notice, error, cancelled. It hides 600 ms after insert, or after the swap resolves.
- [x] The pill window is created at startup and kept hidden, so showing it costs no webview start-up.

## Notes (2026-11-11)

- `pill.rs` is the pure state machine (views, timers, `place`); `overlay.rs` runs it on a thread, sits in front of the webviews' `Emit`, and shows/hides the window with raw Win32 on the UI thread (`SW_SHOWNOACTIVATE`-style `SetWindowPos` with `SWP_NOACTIVATE`; Tauri's own `show` activates). `pill.html` + `Pill.svelte` draw it.
- Tauri sets `WS_EX_APPWINDOW` even with `skipTaskbar`, which puts a window in Alt+Tab; the pill clears it and adds `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` after Tauri's ignore-cursor (`WS_EX_TRANSPARENT | WS_EX_LAYERED`). Live ex-style: `0x80801b8`.
- Polish that fails sends nothing, so Polishing gives up after `POLISH_DEADLINE + 500 ms`. A skipped swap ends as Inserted. `AppEvent::Polished` now carries `swapped`, `AppEvent::Notice` a `hint`.
- Live check (release build, injected Caps Lock into Notepad): Notepad kept focus and its caret throughout; pill bottom-centre 24 px above the taskbar; Listening with resting bars, Transcribing past 250 ms, hidden ~500 ms after an empty result.
- Speech could not be automated: a clip played through the speakers never reaches the laptop mic (echo cancellation).

## Owed by the user

- [ ] Hold Caps Lock in Notepad and speak: bars move with your voice, the partial scrolls off the left, then Inserted (and Polishing → Polished with polish on and a key).
- [ ] Alt+Tab while the pill shows: it isn't listed.
- [ ] On a second monitor with another scale: the pill appears there, the right size.

## Follow-up seen (not ticketed)

- The model unloads after 30 s idle (`idle_unload_secs`); the next hold spends ~2.4 s loading before Listening, the pill shows nothing meanwhile, and speech in that time is lost.
