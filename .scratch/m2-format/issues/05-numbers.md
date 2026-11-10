# 05: Numbers

**What to build:** The numbers pass. It converts English number words in the ADR-0015 number style.

**Blocked by:** 03

**Status:** done

- [x] Cardinals become digits, except one through nine in prose ("three people", "15 people").
- [x] Money, times, dates and percentages always use digits ("$1,250", "5:30 pm", "March 3", "20%"), including spoken forms like "twelve fifty".
- [x] Indian grouping when lakh, crore or rupees is spoken ("₹1,25,000", "2 crore"), Western grouping otherwise ("125,000").
- [x] Hindi number words are left untouched.
- [x] Golden cases cover ambiguity: "one of them", "a couple", and "two" in "me two" stay as spoken.
- [x] The latency gate still passes, and formatted WER is re-measured.

## Comments

- 2026-11-10: Implemented in `crates/bolo-format/src/rules/numbers.rs`, with golden cases in `tests/golden/numbers.toml` and round-trip and idempotence properties in `tests/properties.rs`. Formatted WER is re-measured in BENCH.md §14: Nemotron fmt WER en 9.4% → 9.0%, Deepgram and Hinglish unchanged. Latency p95 is 0.12 ms (debug build).
- Choices beyond the ticket:
  - Outside money, four-digit numbers stay ungrouped ("1500", "2026", "4572"). Five digits and up are grouped.
  - A round lakh, crore, million or billion keeps its word ("2 crore", "$3 million").
  - Four or more single digits spoken in a row join up as a code.
  - "at six thirty" and "six thirty pm" become "6:30". A bare "at five" stays a word.
  - "march" and "may" take a day only as an ordinal.
  - Numerals the engine wrote ("06:30", "4PM") are left as written.
