# Spec: M2, the formatting layer

**Status:** ready-for-agent

This spec builds on `GLOSSARY.md`, ADR-0015 (the Layer 1 rules pipeline) and ADR-0016 (polish in the engine, per segment). It also follows ADR-012 in `docs/DECISIONS.md`: the LLM never blocks the instant path. The scope comes from the "What M2 starts with" section of `docs/M1-REPORT.md` and was grilled with the user on 2026-11-10.

## Problem Statement

Bolo inserts what the recognizer heard, not what the user meant to send:
- Fillers ("um", "matlab") stay in.
- Spoken self-corrections stay in ("Tuesday, actually no, Friday").
- Numbers come out as words ("twelve fifty").
- Spoken lists come out as one run-on sentence.

The only fix today is LLM polish. It adds about a second, it exists only in the CLI, and it is too slow to wait for on long dictations. Two M1 gaps also remain:
- Release → text misses the 180 ms bar end to end, because Stop waits for an in-progress chunk decode.
- Deepgram `multi` sometimes writes Hindi words in Urdu script.

We also can't measure formatting quality. WER against verbatim references counts every correct edit as an error.

## Solution

- **Layer 1 rules on the instant path.** These cover fillers, self-corrections, numbers, lists, the "new line" and "new paragraph" commands, and punctuation fix-ups, all in under 1 ms. The rules are conservative: when unsure, they leave the text verbatim.
- **Polish in the engine.** Polish is off by default. When it's on, it arrives as a second event after the instant text. Long dictations are polished segment by segment while the user is still speaking.
- **Stop no longer waits on a chunk decode**, so release → text meets 180 ms end to end.
- **Urdu-script words are romanized** through the existing Devanagari path.
- **Formatted references** for the 40 clips, so the bench reports formatted WER as well as verbatim WER.
- **An M2 report and `v0.2.0`** (prerelease), followed by a stop for sign-off.

## User Stories

1. As a user, I want "um", "uh" and "like"-style fillers removed from my text, so that it reads as written, not spoken.
2. As a Hinglish user, I want filler-only "matlab" and "haan toh" removed, but real uses of "na" and "woh" kept, so that my meaning isn't changed.
3. As a user, I want "Tuesday, actually no, Friday" to come out as "Friday", so that I can correct myself mid-sentence.
4. As a Hinglish user, I want "nahi nahi" to work as a correction marker, so that corrections work in my language too.
5. As a user, I want Bolo to leave my words alone when it isn't sure a correction was meant, so that it never deletes something I meant to say.
6. As a user, I want "twelve fifty dollars" written as "$1,250" and "fifteen people" as "15 people", while "three people" stays in words, so that numbers look the way I'd type them.
7. As an Indian user, I want "one lakh twenty five thousand rupees" written as "₹1,25,000", so that amounts use the grouping I read.
8. As a user, I want times, dates and percentages as digits ("5:30 pm", "20%"), so that they're scannable.
9. As a user, I want "I need three things: first milk, second eggs, third bread" to become a list, so that I can dictate lists.
10. As a user, I want ordinary sentences joined with "and" to never turn into a list, so that prose stays prose.
11. As a user, I want to say "new line" or "new paragraph" (or "nayi line") to break lines, so that I can lay out a message by voice.
12. As a user, I want the words "comma" and "full stop" kept as words, so that talking about punctuation doesn't break my text.
13. As a user, I want sentence-start capitals, a final period, a capital "I" and clean spacing, so that the instant text looks finished.
14. As a developer dictating into a code editor, I want no automatic capitals or trailing period, so that the text drops into code cleanly.
15. As a user of a Markdown target, I want lists rendered as Markdown lists, so that they display properly.
16. As a user, I want all of this to add no delay I can feel (under 1 ms), so that formatting is free.
17. As a user, I want release → text in 180 ms or less, so that dictation feels instant.
18. As a Deepgram `multi` user, I want Hindi words that Deepgram writes in Urdu script to come out in Roman Hinglish, so that my text has one script.
19. As a user who turns polish on, I want the instant text first and the polished text after it, so that polish never makes me wait.
20. As a user who turns polish on, I want my text left as it was if polish times out, fails or returns something odd, so that polish can only improve things.
21. As a user dictating for a minute, I want the polished text shortly after I stop, not seconds later, so that polish stays useful on long dictations.
22. As a host app developer (desktop or Android), I want polish delivered by the engine as an event, so that I don't reimplement deadlines and guards.
23. As the maintainer, I want every rule covered by golden cases and property tests, so that a regression shows up in CI.
24. As the maintainer, I want the bench to report formatted WER and verbatim WER, for Layer 1 and for Layer 1 + polish, so that I can see what formatting buys.
25. As the maintainer, I want an M2 report and a `v0.2.0` prerelease, so that I can sign off M2 before M3 starts.

## Implementation Decisions

- **Stop path (gap 1).** Find where the 0–190 ms goes, then change the worker so Stop doesn't serialize behind an in-progress chunk decode. One option is to keep the decode off the engine thread. Another is to decode the tail in a way that overlaps the in-flight chunk. The gate is Nemotron English p50 release → text ≤ 180 ms on real-time `bolo replay`, with p95 reported. If it can't be reached, stop and report the numbers. Don't loosen the bar.
- **Rules pipeline (ADR-0015).**
  - A `rules` module in `bolo-format` exposes one entry point. It takes the text, the dictation's language (English/Hinglish) and a small format context (Plain/Markdown, `code_mode`), and returns the formatted text.
  - The worker calls it after transliteration and the loanword lexicon, and its time is `rules_ms`.
  - The passes run in a fixed order: fillers → self-corrections → numbers → lists and commands → punctuation fix-ups → render.
  - `bolo-format` must not depend on `bolo-core`. The format context is its own small type, and `bolo-core` maps `TargetContext` onto it.
- **Number style:**
  - Digits, except one through nine in prose.
  - Money, time, dates and percentages always use digits.
  - Indian grouping when lakh, crore or rupees is spoken, and Western grouping otherwise.
  - English number words only.
- **Urdu script.** An Urdu → Devanagari character map runs before the existing Devanagari → Roman romanizer and loanword lexicon. It's applied wherever Devanagari is handled today, in Roman-script mode.
- **Polish in the engine (ADR-0016).**
  - The engine config gains `polish: Off | On { tone, app, dictionary }`, with `Off` as the default.
  - `EngineEvents` gains `on_polished(FormattedOutput)`. Its `timings.llm_ms` is filled in.
  - The deadline (1.5 s), the meta-reply rule and the length-ratio guard move with polish. On failure the engine emits nothing more.
  - `bolo dictate --polish` becomes a consumer of the event.
- **Per-segment polish.**
  - Applies when polish is on and the dictation reaches 20 s.
  - Silero VAD runs alongside every engine to find pauses of 700 ms or more.
  - The text up to each pause is a segment. For streaming engines that means the stable text up to the pause. Each segment is polished in the background, with the previous polished segment as context.
  - At release, the remaining segment is polished and the parts are joined.
  - Self-corrections that span a pause are a known miss.
- **Formatted references.** Each clip gets a second reference: what the user would send. The agent drafts them and the user corrects them. `bolo run` and `bolo replay` report formatted WER next to verbatim WER. A flag enables a Layer 1 + polish column, because polish costs API money.

## Testing Decisions

- Test external behaviour only: the text out for a given text in, and the events for a given dictation. Don't test pass internals.
- **Golden cases:** TOML files under `crates/bolo-format/tests/golden/`, one per pass plus an end-to-end file, run by a single test that reports each failing case by its `note`.
- **Property tests:**
  - A pass never removes a word that isn't a filler or marker, outside a recognized correction.
  - The pipeline is idempotent.
  - The output is never empty for non-empty input.
- **Latency:** a test or bench enforces < 1 ms p95 for the pipeline on the 40 clips' transcripts.
- **Polish:**
  - Engine tests use the fake STT seam (`crates/bolo-core/tests/common`) plus a fake LLM client. Real polish is behind a feature flag or trait.
  - They check that `on_final` comes before `on_polished`, that a timeout or guard failure gives no `on_polished`, and that per-segment polish starts before release.
  - No network in CI.
- **Stop path:** a fake-STT test with a slow chunk decode shows Stop isn't serialized behind it. The real-time gate is measured with `bolo replay`, not in CI.

## Out of Scope

- HTML output, per-app detection and per-app polish opt-in UI (M3).
- User-defined replacements and a personal dictionary file (M3 settings).
- Hindi number words in Layer 1 (left to polish).
- Spoken punctuation commands.
- A punctuation or truecasing model.
- Self-corrections that span segment boundaries in per-segment polish.

## Further Notes

- Tickets are in `issues/`. Tickets 01, 02, 03, 07 and 08 have no blockers and can start in any order. Ticket 10 is the milestone gate.
- Real-time `bolo replay` is slow. Use `--speed 50` for WER and formatting checks, and real time only for latency gates.
