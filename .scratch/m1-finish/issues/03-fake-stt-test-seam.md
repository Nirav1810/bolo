# 03: Fake STT test seam for the engine

**What to build:** The engine can be built with an injected STT engine, alongside the existing injected audio source. CI can then drive a complete dictation with a scripted fake STT engine and a replayed waveform, with no models and no network. The first tests cover the dictation lifecycle.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] A new engine constructor variant accepts an STT engine. It extends the existing audio-source seam, and production construction is unchanged.
- [ ] Tests can use a scripted fake STT engine. It streams, and it returns preset partials and a preset final for each stream language.
- [ ] CI test: start → feed → stop yields a final containing the scripted text, with timings present.
- [ ] CI test: the dictation state sequence is Idle → Listening → Processing → Done → Idle.
- [ ] CI test: cancel produces no final and returns to Idle.
- [ ] The tests run in `cargo test --workspace` on both CI operating systems without downloading anything.
