# 03: Fake STT test seam for the engine

**What to build:** The engine can be built with an injected STT engine, alongside the existing injected audio source. CI can then drive a complete dictation with a scripted fake STT engine and a replayed waveform, with no models and no network. The first tests cover the dictation lifecycle.

**Blocked by:** 02

**Status:** ready-for-agent

- [x] A new engine constructor variant accepts an STT engine. It extends the existing audio-source seam, and production construction is unchanged.
- [x] Tests can use a scripted fake STT engine. It streams, and it returns preset partials and a preset final for each stream language.
- [x] CI test: start → feed → stop yields a final containing the scripted text, with timings present.
- [x] CI test: the dictation state sequence is Idle → Listening → Processing → Done → Idle.
- [x] CI test: cancel produces no final and returns to Idle.
- [x] The tests run in `cargo test --workspace` on both CI operating systems without downloading anything.

## Comments

**2026-11-09: implemented on `test/fake-stt-seam`**
- Seam: `Engine::with_stt_engine(config, events, audio_factory, stt)` sits next to `with_audio_source`. Both go through one private `spawn`. The worker uses the injected engine in place of building one from `config.engines` (no VAD model). The injected engine survives `update_config`, while the built engine is still rebuilt as before. `Engine::new` and `with_audio_source` are unchanged.
- Fake: `crates/bolo-core/tests/common/mod.rs` has `FakeStt`, a streaming `SttEngine` scripted per stream language (`.script(Language::En, &[partials…], final)`). It reveals one partial per 0.1 s of audio fed and records the language of every stream it opens (`opened()`), so ticket 04 can assert that forced modes never open a second pass. The same module holds a `Harness` (engine + replayed 220 Hz tone + event recorder) with `dictate()`, `start_and_speak()`, `events_until_idle()` and event filters.
- Tests (`crates/bolo-core/tests/dictation.rs`): a final with the scripted text, engine id and timings, with partials streamed in order; the state sequence Idle → Listening → Processing → Done → Idle; cancel gives no final, goes Listening → Idle, and the engine is reusable afterwards.
- Verified: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and `cargo test --workspace` are green on Windows. The dictation tests take about 1.5 s and passed 5 repeat runs in a row. They need no downloads (`async-trait` is the only new dev-dependency, and it was already in the workspace).
