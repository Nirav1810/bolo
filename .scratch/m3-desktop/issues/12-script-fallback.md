# 12: Script fallback for non-Devanagari Indic output

**What to build:** M2 gap 4. In Roman Hinglish mode, output in any Indic script other than Devanagari and Urdu (for example Deepgram `multi` writing hinglish/09 in Telugu) is romanized instead of inserted as is.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `translit::needs_romanizing` detects any Brahmic script block (Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam).
- [ ] The Brahmic blocks share one layout offset from Devanagari, so map them onto Devanagari codepoints by that offset, where a letter exists, and then run the existing romanizer and loanword lexicon. Letters with no Devanagari equivalent get a small explicit table.
- [ ] Golden cases per script, plus the hinglish/09 Deepgram transcript as a regression case.
- [ ] A property test: Roman-mode output contains no Brahmic or Arabic-script characters.
- [ ] Re-run Deepgram `multi` on hinglish with `--speed 50`, and record the result in BENCH.md.
