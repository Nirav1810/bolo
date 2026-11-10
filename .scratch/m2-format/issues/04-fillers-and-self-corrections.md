# 04: Fillers and self-corrections

**What to build:** The filler and self-correction passes (ADR-0015), conservative by design: when unsure, the text stays verbatim.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] English fillers ("um", "uh", "er", "you know" as a filler, repeated words) are removed.
- [ ] Hindi fillers ("matlab", "haan toh", "na", "woh") are removed only in filler-only positions (the start of the dictation, or repeated). Golden cases show real uses kept.
- [ ] Self-corrections act on explicit markers: "actually no", "scratch that", "I mean", "sorry", "nahi nahi". The nearest preceding phrase of the same kind (date, time, number, noun phrase) is replaced.
- [ ] Unsure cases stay verbatim. Golden cases cover markers used as real words ("I mean it", "sorry for the delay").
- [ ] Property test: no word is removed outside the filler list or a recognized correction.
- [ ] The latency gate still passes, and formatted WER is re-measured.
