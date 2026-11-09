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

**Warm mic**:
The window after a dictation during which the microphone stays open so the next dictation loses no opening syllables.
_Avoid_: Hot mic, pre-open

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
Which builds a user receives: **stable** (plain tags like `v0.4.0`) or **beta** (prerelease tags like `v0.4.0-beta.1`).
_Avoid_: Track (Play Store term), ring
