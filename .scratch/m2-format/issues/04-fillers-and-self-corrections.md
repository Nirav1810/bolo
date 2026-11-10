# 04: Fillers and self-corrections

**What to build:** The filler and self-correction passes (ADR-0015), conservative by design: when unsure, the text stays verbatim.

**Blocked by:** 03

**Status:** done

- [x] English fillers ("um", "uh", "er", "you know" as a filler, repeated words) are removed.
- [x] Hindi fillers ("matlab", "haan toh", "na", "woh") are removed only in filler-only positions (the start of the dictation, or repeated). Golden cases show real uses kept.
- [x] Self-corrections act on explicit markers: "actually no", "scratch that", "I mean", "sorry", "nahi nahi". The nearest preceding phrase of the same kind (date, time, number, noun phrase) is replaced.
- [x] Unsure cases stay verbatim. Golden cases cover markers used as real words ("I mean it", "sorry for the delay").
- [x] Property test: no word is removed outside the filler list or a recognized correction.
- [x] The latency gate still passes, and formatted WER is re-measured.

## Comments

**2026-11-10 (agent):** Done.
- The passes are `rules/fillers.rs` and `rules/corrections.rs`. They share the date, time and number classifier in `rules/kinds.rs`, which ticket 05 can reuse.
- Golden cases: `tests/golden/fillers.toml` and `corrections.toml`, plus two end-to-end cases. These include the real Nemotron and Deepgram outputs of en/12 and hinglish/11.
- Property tests:
  - Without a marker, only fillers and stutters are dropped.
  - With a marker spliced in, a correction touches only the corrected sentence and the marker, and never drops a word of the repair.
- Choices made where the ticket was open:
  - **Markers.** I added "nahi wait"/"nahin wait" (bench clip hinglish/11 uses it), and ADR-0015 lists it now. Matching skips punctuation inside a marker ("Actually, no.").
  - **Repair scope.** When a marker starts a sentence, the phrase it corrects can sit in the sentence before (Deepgram writes "Tuesday. Actually, no. Make it Wednesday"). A lead such as "make it", "it's" or "let's say" is dropped before a date, time or number.
  - **Noun phrases.** These are English only, after "actually no", "I mean", "nahi nahi" or "nahi wait". The phrase is at most three words, contains no pronouns, verbs or prepositions, and is not cut out of a phrase after a determiner. "sorry" corrects only dates, times and numbers ("I was wrong, sorry Rahul" is an apology). Hinglish noun phrases are left to polish.
  - **"scratch that".** It corrects a matching phrase. Otherwise it drops the sentence before it, but only when a pause sets it off and it isn't a verb ("scratch that off the list").
  - **Stutters.** Single words only. Numbers, URLs and words repeated on purpose are kept ("bye bye", "very very", "that that"). In Hinglish only a short list of English words collapses, because Hindi repeats words for meaning.
  - **"na"** stays at the start on its own ("Na, main nahi aaunga" means no). "na na" stays too. "woh" at the start goes only before a pause.
  - **"like"** goes only between two commas.
  - The filler pass runs until nothing changes, which keeps the pipeline idempotent when one removal sets up another.
- Formatted WER was re-measured (BENCH.md §13). Only en/12 (both engines) and hinglish/11 (Deepgram) changed, and all three now match their formatted references. English formatted WER is 11.3% → 9.4% (Nemotron) and 7.4% → 5.5% (Deepgram). Latency gate p95 is 0.065 ms (debug build).
