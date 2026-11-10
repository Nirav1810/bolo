# 09: Per-segment polish for long dictations

**What to build:** When polish is on and a dictation reaches 20 s, it is polished one **segment** at a time while the user keeps speaking (ADR-0016). Release then only waits for the last segment.

**Blocked by:** 08

**Status:** done

- [x] When polish is on, Silero VAD runs alongside every engine, streaming engines included. When polish is off it costs nothing.
- [x] After 20 s, each pause of 700 ms or more closes a segment: the stable text up to the pause. The segment is polished in the background, with the previous polished segment as read-only context.
- [x] At release, the remaining text is polished and joined with the earlier segments. A segment that failed or timed out keeps its Layer 1 text.
- [x] Dictations under 20 s are polished once, as a whole (ticket 08 behaviour).
- [x] Engine tests (fake STT + fake LLM): segment polish starts before release, the joined text is in order, and a failed segment falls back.
- [x] Measured: release → `on_polished` for a dictation of 60 s or longer, compared with whole-text polish, recorded in BENCH.md.

**Done (2026-11-10):**
- Built in `bolo-core/src/segment.rs`: `PauseDetector` (Silero, or an energy gate without the model) and `Polisher`, one task per dictation that polishes segments in order with the previous one as `<context>`.
- Wired up in `worker.rs`.
- Engine tests are in `crates/bolo-core/tests/segment_polish.rs`.
- Measured with `bolo polish-latency`: release → `on_polished` p50 128 ms per segment against 1496 ms for whole text, on 61–67 s dictations. Details in BENCH.md §17.
