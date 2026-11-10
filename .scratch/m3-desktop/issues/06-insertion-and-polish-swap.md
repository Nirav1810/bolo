# 06: Clipboard insertion and the polish swap

**What to build:** Insertion and the polish swap from ADR-0018: paste the instant text into the target app, restore the user's clipboard, and swap in polished text when it is safe.

**Blocked by:** 01, 03, 05

**Status:** ready-for-agent

- [ ] Insert:
  - save `CF_UNICODETEXT`, `CF_HTML`, `CF_DIBV5` and `CF_HDROP`;
  - write the text (plus `CF_HTML` with the correct header offsets for HTML targets) with the clipboard-history and monitor exclusion formats;
  - send Ctrl+V, or Shift+Insert per app rule;
  - restore after 300 ms if the sequence number is still ours.
- [ ] A UIPI-blocked target (the foreground process is elevated and Bolo isn't) → leave the text on the clipboard and send a notice event to the pill.
- [ ] Swap decision as a pure function, `swap_decision(touched, same_window, age, multiline, app_multiline_safe) -> Swap | Skip(reason)`, with a 5 s age limit. Table-tested.
- [ ] Swap: Shift+Left × the caret length (grapheme clusters, `\r\n` = 1), then paste the polished text. The `WH_MOUSE_LL` hook is installed only while a swap is pending.
- [ ] A new dictation, or a cancel, drops any pending swap.
- [ ] Instrumentation: key-up → paste-sent time is logged per dictation, next to the engine's `total_ms`.
- [ ] Controller tests with the fake `Desktop`: insert order, the clipboard restore rule, swap and skip paths, a superseded swap.
- [ ] Manual: Notepad, Word, Chrome (Gmail compose), Slack, VS Code, Windows Terminal, and an elevated Notepad.
