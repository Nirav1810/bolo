# 07: Urdu-script words to Roman Hinglish

**What to build:** Deepgram `multi` sometimes writes Hindi words in Urdu script ("تیم" for "team"). Map Urdu script to Devanagari, then reuse the existing Devanagari → Roman romanizer and loanword lexicon, so Roman-script output contains no Urdu script.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] An Urdu → Devanagari character map lives in `bolo-format` and runs wherever Devanagari is handled in Roman-script mode.
- [ ] Unit tests: the observed cases ("تیم" → "team") plus common Hindi function words.
- [ ] Property test: Roman-script output never contains Arabic-script characters.
- [ ] Deepgram `multi` Hinglish WER is re-measured, and the gap is closed in BENCH.md.
