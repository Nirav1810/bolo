# 01: Stop no longer waits on an in-progress chunk decode

**What to build:** Today Stop is handled only after the engine thread finishes the chunk decode in progress, which costs 0–190 ms. That's gap 1 in `docs/M1-REPORT.md`. Measure where the time goes, then change the worker so release → text meets the 180 ms bar end to end.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] The release → text time is broken down per stage and recorded in the ticket comments: Stop received, waiting for the decode, the tail decode, and formatting.
- [ ] The worker no longer handles Stop only after a chunk decode finishes.
- [ ] CI test (fake STT seam): with a deliberately slow chunk decode, a final still arrives sooner than serialized handling would allow, and the text is still complete.
- [ ] Gate: Nemotron English p50 release → text ≤ 180 ms on real-time `bolo replay` over the 40 clips, with p95 reported. If it misses, stop and report the numbers. Don't loosen the bar.
- [ ] Auto mode's p50 and p95 are re-measured, and BENCH.md is updated.
