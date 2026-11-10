# 08: Overlay pill

**What to build:** The overlay pill from the approved design (ticket 04): a transparent, always-on-top, non-activating, click-through window that shows the dictation's state, a live waveform and the partial text.

**Blocked by:** 01, 04

**Status:** ready-for-agent

- [ ] The window never takes focus or receives clicks (`WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW`). Not in the taskbar or Alt+Tab. Verified by dictating into Notepad with the caret still blinking.
- [ ] Placed bottom-centre on the monitor of the window in front, DPI-aware.
- [ ] Waveform from `on_level`, coalesced to one update per frame. Partials on one line, truncated from the left.
- [ ] States from the design: listening, processing, polishing, inserted, notice, error, cancelled. It hides 600 ms after insert, or after the swap resolves.
- [ ] The pill window is created at startup and kept hidden, so showing it costs no webview start-up.
