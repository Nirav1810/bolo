# Insert through the clipboard; swap in polish by selecting back

**Insertion.** The desktop app inserts text by pasting: it writes the clipboard and sends Ctrl+V with `SendInput`. Apps listed in the app rules get Shift+Insert instead. Pasting is the only method that is fast for long text, works in nearly every Windows app, and carries HTML (`CF_HTML`) to targets that render it (Word, Outlook, Gmail).

To keep the user's clipboard intact:
- Save `CF_UNICODETEXT`, `CF_HTML`, `CF_DIBV5` and `CF_HDROP` before writing. Other formats (for example an Excel range's private formats) are lost, and the docs say so.
- Mark Bolo's write with `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory` = 0, so dictations stay out of Win+V history and clipboard managers.
- Restore the saved contents after 300 ms, but only if the clipboard sequence number is still Bolo's. If the user copied something in between, theirs wins.
- If the paste can't reach the window (an elevated target under UIPI), leave the text on the clipboard and tell the user in the overlay pill.

**Polish swap.** When `on_polished` arrives after the instant text was pasted, Bolo selects the instant text back with Shift+Left, once per caret unit, then pastes the polished text. A caret unit is a grapheme cluster, with `\r\n` counted as one. Bolo swaps only when all of these hold:
- no key or mouse click since the insert (the keyboard hook, plus a mouse hook installed only while a swap is pending);
- the same foreground window;
- the polish arrived within 5 s of the insert;
- the text is one line, or the app is marked multi-line-safe in the app rules. Editors disagree on how a pasted line break or list item counts against Shift+Left.

Otherwise the instant text stays, and history records the polished text with the reason for the skip.

## Considered Options

- **Undo, then paste**: rejected. Undo granularity differs per app. Word, browsers and Slack may undo more or less than the paste.
- **Wait for polish when it's on**: rejected. It puts the LLM in the blocking path (ADR-012).
- **Type the text with `SendInput` characters**: rejected. It is slow for long text, garbled by IME and autocomplete, and can't carry formatting.
- **UI Automation `ValuePattern`/`TextPattern`**: rejected for M3. Support is patchy (Electron, terminals), and it replaces the whole field rather than inserting at the caret.
