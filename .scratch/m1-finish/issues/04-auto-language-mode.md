# 04: Auto language mode

**What to build:** The Auto language mode from ADR-0014, which becomes the default if it meets the shipping bar below.
- **Two passes.** Each dictation is decoded as English and as Hindi in parallel, on the one loaded model.
- **One choice per dictation.** At release, one result is chosen for the whole dictation, based on how dense Hindi function words are in the Hindi pass.
- **Ambiguous dictations** reuse the previous dictation's language, or English if there is no previous dictation.
- **Live preview** shows the pass that is currently favoured.
- **Forced modes.** English and Hinglish modes run a single pass.
- **Cloud engines** in Auto use their native multilingual mode.

**Blocked by:** 03

**Status:** done

- [x] Engine config has a language mode: Auto, English or Hinglish.
  - Auto is the default only if the shipping bar below is met.
  - Otherwise the default stays English and Auto is opt-in.
- [x] Streaming local engines that have a per-stream language option (Nemotron) run two passes. Forced modes run a single pass.
- [x] CI tests through the fake STT seam check that:
  - English text chooses English;
  - Hindi text rich in function words chooses Hinglish;
  - an ambiguous dictation reuses the previous language;
  - a first ambiguous dictation chooses English;
  - a forced mode never runs a second pass;
  - partials follow the favoured pass.
- [x] Deepgram in Auto mode sends `language=multi`.
- [x] `bolo replay` accepts `--lang auto|en|hi`.
- [x] Shipping bar on the 40-clip set:
  - English WER ≤ 8.5%;
  - Hinglish WER ≤ 30%;
  - release→text p50 ≤ 180 ms.

  The results and the tuned thresholds are recorded in docs/BENCH.md.

## Comments

- 2026-11-09: Implemented. Auto picks the right language on 40/40 clips (EN WER 8.0%, Hinglish 27.7%). Latency is at parity with the forced modes. The literal 180 ms bar is missed on `bolo replay`'s end-to-end clock by every mode, forced ones included. The user chose to keep Auto as the default (see docs/BENCH.md §7). Follow-up worth a ticket: the engine thread blocks Stop while it decodes a chunk (0–190 ms).
