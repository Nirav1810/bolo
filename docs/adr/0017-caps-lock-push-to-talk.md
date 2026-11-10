# Caps Lock is the push-to-talk key; a tap still toggles caps

The desktop app's default **push-to-talk key** is Caps Lock, held. Few people use it in daily typing, it sits under the left little finger, and holding it has no meaning today.

- A `WH_KEYBOARD_LL` hook swallows every Caps Lock down and up, so holding it never changes the caps state.
- A hold of **150 ms** or more starts a dictation, and releasing the key stops it. The warm mic's pre-roll (ADR-009) covers the 150 ms, so the first syllable isn't lost.
- A release before 150 ms is a **tap**. Bolo replays a real Caps Lock press with `SendInput`, tagged through `dwExtraInfo` so the hook ignores its own keystroke, and caps toggles as usual. A tap costs about 150 ms of delay on the caps toggle, nothing else.
- Caps Lock with any modifier held (Shift, Ctrl, Alt, Win) passes through untouched. This is the always-available escape hatch.
- Esc is swallowed only while a dictation is active, and it cancels the dictation.
- Users who need Caps Lock as a held key can choose Right Ctrl, Right Alt or F13–F24 instead.

The hold/tap logic is a pure state machine with an injected clock. The hook procedure only posts events to it, because Windows drops a low-level hook whose callback exceeds `LowLevelHooksTimeout`.

## Considered Options

- **Hold Right Ctrl**: fine, but used in some shortcuts and awkward on laptops that lack it.
- **Ctrl+Win (Wispr Flow's default)**: two keys, and it collides with Win+key shortcuts.
- **Right Alt**: it is AltGr on many Indian and European layouts, which those users need.
- **Start the dictation on key-down and cancel it on a tap**: rejected. The cancel would supersede a pending polish of the previous dictation (ADR-0016), and the warm-mic pre-roll already makes the 150 ms free.
- **Double-tap for hands-free**: deferred. It would delay every single tap by about 250 ms while Bolo waits for a possible second tap.
