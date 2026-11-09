# 05: `bolo dictate` on the engine

**What to build:** `bolo dictate` runs on the same engine the apps will use, with its warm mic, pre-roll, VAD segment-ahead decoding and language mode. It consumes the engine's events:
- Enter starts and stops a dictation.
- Partials appear live.
- The final text goes to the clipboard.

With `--polish`, the LLM runs only after the instant text is already on the clipboard, then replaces it (ADR-012).

**Blocked by:** 02

**Status:** ready-for-human (only the manual speech check is left)

- [x] The old bespoke capture/resample/stream pipeline is removed from `dictate`, which drives only the engine.
- [x] `--lang auto|en|hi` is supported. The default is auto, which passes through unchanged until ticket 04 lands.
- [x] `-e/--engine` accepts the same specs as today (local ids, deepgram, groq, sarvam).
- [x] The instant text and the polished text are shown separately, each with its latency. API keys are never printed.
- [ ] Manually verified: several dictations in a row with the warm mic, in English and in Hinglish. Scripted runs on the live mic (start, stop, cancel, two dictations in a row on a warm mic; nemotron-stream in Auto, parakeet-v2-en with VAD plus `--polish`, deepgram) passed. Speech in English and Hinglish still needs a human.
