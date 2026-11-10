# 07: Urdu-script words to Roman Hinglish

**What to build:** Deepgram `multi` sometimes writes Hindi words in Urdu script ("تیم" for "team"). Map Urdu script to Devanagari, then reuse the existing Devanagari → Roman romanizer and loanword lexicon, so Roman-script output contains no Urdu script.

**Blocked by:** none

**Status:** done

- [x] An Urdu → Devanagari character map lives in `bolo-format` and runs wherever Devanagari is handled in Roman-script mode.
- [x] Unit tests: the observed cases ("تیم" → "team") plus common Hindi function words.
- [x] Property test: Roman-script output never contains Arabic-script characters.
- [x] Deepgram `multi` Hinglish WER is re-measured, and the gap is closed in BENCH.md.

## Outcome

- `bolo_format::urdu` (word table + letter map + loanword cluster/-ing heuristics) runs inside `devanagari_to_roman_with`, so every Roman-script path gets it. Callers check `translit::needs_romanizing`. The Hinglish detector maps Urdu before counting function words.
- Re-measured with `deepgram-stream` (BENCH.md §16). No Urdu came back this run, so WER is unchanged (hinglish 22.8%). The gap is closed by the map and its tests.
- Follow-up found: Deepgram `multi` wrote one Hinglish clip (hinglish/09) entirely in Telugu script. Needs its own ticket.

