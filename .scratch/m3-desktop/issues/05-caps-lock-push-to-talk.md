# 05: Caps Lock push-to-talk hook

**What to build:** The `WH_KEYBOARD_LL` hook and the hold/tap state machine from ADR-0017, driving `start_dictation`, `stop_dictation` and `cancel_dictation` with the target context of the window in front.

**Blocked by:** 01

**Status:** ready-for-human (the physical-keyboard check below)

- [x] The hook runs on its own thread with a message loop. The callback only classifies the key and posts it, with no locks and no allocation on the hot path.
  - The machine itself runs inside the callback (a `thread_local` `RefCell`, no lock, no allocation), because whether to swallow the key must be decided before the callback returns. Commands are posted to the hook thread's own queue as `WM_APP` messages, then forwarded to a separate command thread, so a model load on Start never stalls the hook.
- [x] A pure state machine (`hotkey.rs`) with an injected clock. Events: KeyDown/KeyUp(key, modifiers, injected), Tick. Outputs: Start, Stop, Cancel, ReplayTap, PassThrough, Swallow.
- [x] Caps Lock: swallowed, a hold of 150 ms or more → Start, release → Stop, a release under 150 ms → ReplayTap (`SendInput` Caps Lock with a Bolo `dwExtraInfo` tag). Auto-repeat ignored. With any modifier held → PassThrough.
- [x] Esc while Listening → Cancel (swallowed). Esc otherwise → PassThrough.
- [x] The key comes from settings: Caps Lock, Right Ctrl, Right Alt, F13–F24. "Pause Bolo" in the tray unhooks it.
  - Only `push_to_talk` is read from `bolo-store` settings so far, at startup; the rest is wired in tickets 07 and 09. With Right Alt, the fake Left Ctrl that AltGr layouts send is not counted as a modifier.
- [x] Pre-roll check: confirm that the warm mic's pre-roll ring holds at least 300 ms. If it doesn't, raise it in `bolo-core`, so the 150 ms threshold loses no speech. Cold mic is documented as ADR-009's case.
  - `PREROLL_SECS` is 0.3; a compile-time assert in `bolo-core/src/worker.rs` keeps it there.
- [x] Every key or click while a polish swap is pending sets the controller's "touched" flag (used by ticket 06).
  - `hotkey::Flags`: ticket 06 calls `flags.watch_touch()` and then `KeyHook::watch_changed()`, which installs the `WH_MOUSE_LL` hook until `stop_watching()` + `watch_changed()`.
- [x] State machine tests with a fake clock: tap, hold, repeat, Shift+Caps, Esc active/idle, injected ignored, a hold across the 150 ms boundary.
- [ ] Manual: Caps Lock still toggles on a tap in Notepad; holding it dictates; the caps LED never flips during a hold.
  - Checked on 2026-11-11 with synthesized keys (`keybd_event` from a separate process) against the debug build: a tap toggled Caps Lock through the replay, a 1.3 s hold with auto-repeat dictated and left the Caps state unchanged, Esc cancelled, and Shift+Caps toggled. Still owed: the same check on the physical keyboard, watching the LED.
