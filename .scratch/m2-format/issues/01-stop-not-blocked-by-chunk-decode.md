# 01: Stop no longer waits on an in-progress chunk decode

**What to build:** Today Stop is handled only after the engine thread finishes the chunk decode in progress, which costs 0–190 ms. That's gap 1 in `docs/M1-REPORT.md`. Measure where the time goes, then change the worker so release → text meets the 180 ms bar end to end.

**Blocked by:** none

**Status:** done

- [x] The release → text time is broken down per stage and recorded in the ticket comments: Stop received, waiting for the decode, the tail decode, and formatting.
- [x] The worker no longer handles Stop only after a chunk decode finishes.
- [x] CI test (fake STT seam): with a deliberately slow chunk decode, a final still arrives sooner than serialized handling would allow, and the text is still complete.
- [x] Gate: Nemotron English p50 release → text ≤ 180 ms on real-time `bolo replay` over the 40 clips, with p95 reported. If it misses, stop and report the numbers. Don't loosen the bar.
- [x] Auto mode's p50 and p95 are re-measured, and BENCH.md is updated.

## Comments

- 2026-11-10: Implemented. The full write-up is in docs/BENCH.md §10.
  - **Breakdown before** (`main`, 15 English clips, real time):
    - Stop waited 0–88 ms for an in-flight chunk decode (0 on 10 of 15 clips);
    - hand-over ~0.1 ms;
    - tail decode 150–186 ms, almost always 2 chunk decodes of the fixed 0.6 s pad;
    - rules < 0.01 ms.
  - **Breakdown after** (40 clips):
    - Stop waits ~0.02 ms;
    - tail decode p50 107 / p95 237 ms, with 0 decodes on 11 clips, 1 on 24 and 2 on 5;
    - rules < 0.1 ms.
  - **Fix:**
    - each streaming pass decodes on its own thread;
    - the tail pad ends at the chunk holding the end of speech + 480 ms, using a geometry probe at load and a −46 dB speech-end tracker.
  - **Gate:** Nemotron English is 107 / 220 ms (p50 / p95), against `main` at 215 / 313 ms in a back-to-back run. Auto is 129 / 267 ms, against 229 / 350 ms. WER is unchanged (en 8.0%, Hinglish 27.7%), and Auto still picks the right language on 40/40. Bar met.
  - **CI:** `crates/bolo-core/tests/stop_path.rs`. Both tests fail on the old worker.
