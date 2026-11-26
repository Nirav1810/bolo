# 06: The voice keyboard (InputMethodService)

**What to build:** Bolo as a system keyboard that dictates into any app, per ADR-0022 and the approved mockups.

**Blocked by:** 01, 03, 04

**Status:** ready-for-agent

- [ ] `BoloIme : InputMethodService` with a Compose panel (via `AbstractComposeView` and the lifecycle owners IMEs need), declared with `BIND_INPUT_METHOD` and an `input-method` XML.
- [ ] Mic button: a tap starts and the next tap stops; hold > 300 ms is push-to-talk and stops on release. Cancel (×) discards. A waveform from `on_level`. Panel states as mocked: idle, listening, loading model, finishing, polishing, error, sensitive field.
- [ ] Insertion: partials → `setComposingText`; final → `commitText`; polish → replace in place only if the committed text still sits just before the cursor (tracked via `onUpdateSelection`), inside a batch edit. Text is formatted for the target (plain; Markdown where the app row says so).
- [ ] Keys: space, backspace (repeat on hold, deletes composing text first), enter honouring `imeOptions` (`performEditorAction` for send/search/go/next/done, else a newline), switch keyboard (`switchToPreviousInputMethod`; long-press → system IME picker).
- [ ] Lifecycle: `Bolo` created lazily on first tap; the mic closes in `onFinishInputView`; `onTrimMemory` releases the model; the engine is held by the IME service, not a static, so a killed process restarts cleanly. No foreground service, job or wakelock.
- [ ] Mic permission: if it isn't granted, the panel offers "Open Bolo to allow the microphone". An IME can't show the runtime permission dialog itself.
- [ ] Haptics on start and stop (respecting the system setting).
- [ ] Tests: the pure insertion decision (Kotlin unit test: replace or drop polish given the committed text and cursor history), and an instrumented test that types into an `EditText` through the IME with the fake STT (`Bolo` test constructor) on the emulator.

## Human: real apps

- [ ] On the emulator, in Chrome's address bar, Gmail compose (or another mail app), Google Messages, and Keep/Notes: dictate a sentence, then a Hinglish sentence. The text lands, enter does the field's action, and polish replaces in place.
- [ ] Type a word with Gboard right after dictating with polish on: the polish is dropped and your word is kept.
- [ ] A password field shows the notice and never records.
