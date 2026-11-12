# Bolo

Bolo turns speech (English or Hinglish) into polished text typed into whatever app the user is in, locally first and with optional cloud engines.

## Dictation

**Dictation**:
One hold-to-talk (or tap-to-talk) utterance, from key press to text inserted. The unit every language, latency and formatting decision is made for.
_Avoid_: Session, recording, utterance

**Instant path**:
The text a dictation produces without waiting on any network LLM; it is what gets inserted at key release.
_Avoid_: Fast path, raw output

**Polish**:
An optional LLM rewrite of a dictation's text (punctuation, fillers, tone) that never blocks the instant path.
_Avoid_: Cleanup, LLM pass, formatting (formatting covers all layers)

**Segment**:
A stretch of one dictation between pauses of at least 700 ms, found by VAD. Long dictations are polished one segment at a time while the user keeps speaking (ADR-0016).
_Avoid_: Chunk (the STT engine's fixed-size decode unit), utterance

**Formatted reference**:
The text the user would actually send for a benchmark clip, with fillers removed, corrections applied and numbers as digits. Scored alongside the verbatim reference, which is what was literally said.
_Avoid_: Golden output (golden cases are the rules' unit tests)

**Warm mic**:
The window after a dictation during which the microphone stays open so the next dictation loses no opening syllables.
_Avoid_: Hot mic, pre-open

**Input device**:
The microphone Bolo records from: the one the user chose by name (`[audio] input_device`), or the system default. When the chosen one isn't plugged in, Bolo falls back to the default and says so once; it's resolved again on every open, so a re-plugged mic is used again.
_Avoid_: Audio device (could be an output), source (the engine's `AudioSource` also covers WAV replay)

## Desktop

**Push-to-talk key**:
The key the user holds to dictate on the desktop: Caps Lock by default (ADR-0017). A short tap of it is passed on as a normal key press.
_Avoid_: Hotkey (fine in code, but ambiguous in docs), trigger

**Target app**:
The foreground app the text will be inserted into. Its exe (and window title for browsers) decides the dictation's `TargetContext`: format, `code_mode` and whether polish runs, from the bundled app rules and the user's app profile for it.
_Avoid_: Destination, focus app

**App profile**:
The user's choices for one app, edited on the Apps tab and stored as an `[[apps]]` row in settings: its name, polish on or off, tone and format. Each field the user set wins over the bundled app rule's (ADR-0020).
_Avoid_: Override, per-app polish

**Snippet**:
A saved trigger phrase and the text it expands to, kept as a `[[snippets]]` row in settings. Saying the trigger inserts the text verbatim in its place; it is expanded last, after Layer 1 and after polish, and polish is told to keep the trigger as spoken.
_Avoid_: Macro, shortcut, text replacement

**Overlay pill**:
The small, non-activating, click-through window that shows listening state, the waveform and partial text during a dictation.
_Avoid_: HUD, toast, popup

**Polish swap**:
Replacing already-inserted instant text with its polished version, by selecting it back and pasting. It happens only when the user hasn't touched the keyboard or mouse in the same window since the insert (ADR-0018).
_Avoid_: Replace, auto-correct

## Language

**Hinglish**:
Hindi-English mixed speech, written in Roman script by default (e.g. "kal meeting hai, please confirm").
_Avoid_: Hindi (means Devanagari-only Hindi), code-mixed

**Language mode**:
The user's setting for which language a dictation is in: **Auto**, **English**, or **Hinglish**. Auto decides once per dictation; when it can't tell, it reuses the previous dictation's language (English the first time).
_Avoid_: Locale, language setting

## Releases

**Product version**:
The single version number shared by the Windows app, the Android app and the core crates (e.g. `v0.4.0`).
_Avoid_: App version, build number (Android's versionCode is derived from it)

**Release channel**:
Which builds an install receives (ADR-0019): **Bolo**, the `vX.Y.Z` releases (tagged Beta until 1.0), or **Bolo Nightly**, built from every push to `main` and versioned like `0.4.0-nightly.20261111.7`. Each channel is its own app; they install side by side.
_Avoid_: Track (Play Store term), ring, alpha

**Bolo Nightly**:
The separate Windows app built from `main` (violet icon, own install folder), published to the rolling `nightly` GitHub prerelease. It shares settings, models and history with Bolo; its version is never committed.
_Avoid_: Dev build, alpha, canary
