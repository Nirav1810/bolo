# Android: a voice-only keyboard that types through composing text

Bolo on Android is an `InputMethodService` whose panel is for voice only. It has a mic button, cancel, space, backspace, the field's enter action, and a button that switches back to the previous keyboard (`switchToPreviousInputMethod`, with `shouldOfferSwitchingToNextInputMethod` deciding whether to show the globe). It has no letter keys. The user keeps Gboard (or any keyboard) for typing and switches to Bolo to dictate.

**Insertion.** Partials go in as composing text (`setComposingText`), so the app shows them underlined and replaces them each time. The final Layer 1 text is committed with `commitText`. Polish replaces in place, but only if it's safe. The IME remembers what it committed and where (`onUpdateSelection` gives the cursor). When polish arrives and the text just before the cursor still equals that commit, the IME deletes it (`deleteSurroundingText`) and commits the polished text. Otherwise the user has moved on and polish is dropped, which is ADR-0018's "only if untouched" rule in Android terms. The whole replace runs inside `beginBatchEdit`/`endBatchEdit`.

**Per-app profiles.** The app key is `EditorInfo.packageName` instead of the exe name. `[[apps]]` rows match either one, so a single settings schema serves both platforms (ADR-0020). Fields marked as passwords (`TYPE_TEXT_VARIATION_*PASSWORD`, `TYPE_NUMBER_VARIATION_PASSWORD`) or flagged `IME_FLAG_NO_PERSONALIZED_LEARNING` never record and never write history.

**Lifecycle.** The engine lives in the IME process and is created lazily, at the first mic tap after the process starts. The mic opens on a tap and stays warm for `mic_warm_secs` only while the keyboard is shown. `onFinishInputView` closes it at once. The model unloads after `idle_unload_secs = 20` and on `onTrimMemory(TRIM_MEMORY_BACKGROUND)` or higher. Nothing runs while the keyboard is hidden: no service, no job, no wakelock.

## Considered Options

- **A full QWERTY keyboard with a mic key:** rejected for M5. Layouts, autocorrect and swipe are a product of their own, and users already have a typing keyboard they like.
- **A floating bubble over any keyboard (accessibility service + overlay):** rejected. It matches Wispr Flow's look, but Play review of accessibility services is strict, the service runs all the time (battery), and accessibility text insertion is less reliable than an `InputConnection`.
- **Commit partials as real text and patch them:** rejected. Apps would see every partial as an edit (send buttons enable, drafts sync), and undoing a wrong partial would need the same select-back as desktop.
