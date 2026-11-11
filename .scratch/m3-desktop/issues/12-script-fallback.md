# 12: Script fallback for non-Devanagari Indic output

**What to build:** M2 gap 4. In Roman Hinglish mode, output in any Indic script other than Devanagari and Urdu (for example Deepgram `multi` writing hinglish/09 in Telugu) is romanized instead of inserted as is.

**Blocked by:** none

**Status:** done

- [x] `translit::needs_romanizing` detects any Brahmic script block (Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam).
- [x] The Brahmic blocks share one layout offset from Devanagari, so map them onto Devanagari codepoints by that offset, where a letter exists, and then run the existing romanizer and loanword lexicon. Letters with no Devanagari equivalent get a small explicit table.
- [x] Golden cases per script, plus the hinglish/09 Deepgram transcript as a regression case.
- [x] A property test: Roman-mode output contains no Brahmic or Arabic-script characters.
- [x] Re-run Deepgram `multi` on hinglish with `--speed 50`, and record the result in BENCH.md.

**Result (2026-11-11):** `bolo_format::brahmic` maps the eight Brahmic blocks onto Devanagari by offset. An explicit table covers the letters that don't map by offset, and anything left over is dropped. `translit::to_devanagari` now runs both the Urdu and the Brahmic map, and the romanizer and the Hinglish detector use it. Tests are in `crates/bolo-format/tests/brahmic.rs`. BENCH.md §19 has the numbers: hinglish/09 now comes out Roman, and its CER went from 100% to 42%. The live `--speed 50` batch replay showed no third script, but it did drop leading words at 50×. That's logged as a follow-up.
