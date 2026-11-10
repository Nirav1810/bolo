# 05: Caps Lock push-to-talk hook

**What to build:** The `WH_KEYBOARD_LL` hook and the hold/tap state machine from ADR-0017, driving `start_dictation`, `stop_dictation` and `cancel_dictation` with the target context of the window in front.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] The hook runs on its own thread with a message loop. The callback only classifies the key and posts it, with no locks and no allocation on the hot path.
- [ ] A pure state machine (`hotkey.rs`) with an injected clock. Events: KeyDown/KeyUp(key, modifiers, injected), Tick. Outputs: Start, Stop, Cancel, ReplayTap, PassThrough, Swallow.
- [ ] Caps Lock: swallowed, a hold of 150 ms or more → Start, release → Stop, a release under 150 ms → ReplayTap (`SendInput` Caps Lock with a Bolo `dwExtraInfo` tag). Auto-repeat ignored. With any modifier held → PassThrough.
- [ ] Esc while Listening → Cancel (swallowed). Esc otherwise → PassThrough.
- [ ] The key comes from settings: Caps Lock, Right Ctrl, Right Alt, F13–F24. "Pause Bolo" in the tray unhooks it.
- [ ] Pre-roll check: confirm that the warm mic's pre-roll ring holds at least 300 ms. If it doesn't, raise it in `bolo-core`, so the 150 ms threshold loses no speech. Cold mic is documented as ADR-009's case.
- [ ] Every key or click while a polish swap is pending sets the controller's "touched" flag (used by ticket 06).
- [ ] State machine tests with a fake clock: tap, hold, repeat, Shift+Caps, Esc active/idle, injected ignored, a hold across the 150 ms boundary.
- [ ] Manual: Caps Lock still toggles on a tap in Notepad; holding it dictates; the caps LED never flips during a hold.
