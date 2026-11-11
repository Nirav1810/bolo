# 06: Clipboard insertion and the polish swap

**What to build:** Insertion and the polish swap from ADR-0018: paste the instant text into the target app, restore the user's clipboard, and swap in polished text when it is safe.

**Blocked by:** 01, 03, 05

**Status:** ready-for-human (the manual app checks below)

- [x] Insert:
  - save `CF_UNICODETEXT`, `CF_HTML`, `CF_DIBV5` and `CF_HDROP`;
  - write the text (plus `CF_HTML` with the correct header offsets for HTML targets) with the clipboard-history and monitor exclusion formats;
  - send Ctrl+V, or Shift+Insert per app rule;
  - restore after 300 ms if the sequence number is still ours.
- [x] A UIPI-blocked target (the foreground process is elevated and Bolo isn't) → leave the text on the clipboard and send a notice event to the pill.
- [x] Swap decision as a pure function, `swap_decision(touched, same_window, age, multiline, app_multiline_safe) -> Swap | Skip(reason)`, with a 5 s age limit. Table-tested.
- [x] Swap: Shift+Left × the caret length (grapheme clusters, `\r\n` = 1), then paste the polished text. The `WH_MOUSE_LL` hook is installed only while a swap is pending.
- [x] A new dictation, or a cancel, drops any pending swap.
- [x] Instrumentation: key-up → paste-sent time is logged per dictation, next to the engine's `total_ms`.
- [x] Controller tests with the fake `Desktop`: insert order, the clipboard restore rule, swap and skip paths, a superseded swap.
- [ ] Manual: Notepad, Word, Chrome (Gmail compose), Slack, VS Code, Windows Terminal, and an elevated Notepad.

**Done (agent):** `src-tauri/src/insert.rs` (the `Inserter`, `swap_decision`, `caret_units`, `cf_html`), the Win32 clipboard, `SendInput` and the integrity-level (UIPI) check in `desktop.rs`, and a `notice` app event. The paste chord and `multiline_swap_safe` take their defaults (Ctrl+V, not safe) until ticket 07's app rules. Verified live on this machine with the `#[ignore]`d tests: a clipboard round trip, plus a paste, a swap over Devanagari text and a restore into an EDIT control. The manual list above is still open: real apps and an elevated Notepad.
