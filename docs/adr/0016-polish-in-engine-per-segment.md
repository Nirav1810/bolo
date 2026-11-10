# Polish runs in the engine, per segment for long dictations

This amends ADR-012 in `docs/DECISIONS.md`: the LLM still never blocks the instant path.

Until now, **polish** was wired up only in the CLI's `dictate` loop. It moves into `bolo-core`, so the desktop app (M3) and Android (M5) get the same behaviour:

- The engine first emits `on_final` with the Layer 1 text, which is the **instant path**. Later it emits `on_polished` with the polished text, or does not emit it at all when polish was skipped, timed out or failed a guard.
- Polish is **off by default**. The engine config enables it per dictation (`polish: On { tone, app, dictionary }`) and needs the Anthropic key in `ApiKeys`. The engine takes the flag, and choosing which apps opt in is the host app's job (M3).
- The 1.5 s deadline, the meta-reply prompt rule and the length-ratio guard stay. On any failure the Layer 1 text stands, and no `on_polished` is sent.

**Per-segment polish.** For dictations of **20 s or longer**, waiting to polish the whole text at release would be felt.
- When polish is on, Silero VAD runs alongside every engine, streaming engines included, to find pauses of **700 ms or more**.
- The text up to each pause is a **segment**. It is polished while the user keeps talking, with the previous polished segment sent as read-only context.
- At release only the last segment is still waiting, so `on_polished` arrives about one short polish call after release.
- Shorter dictations are polished once, as a whole.

A self-correction that spans a pause ("Tuesday … [pause] … actually no, Friday") is not fixed across segments. We accept this: Layer 1 already handles the common case within a segment.

## Considered Options

- **Keep polish in each host app**: rejected. It would mean three copies of the deadline, guard and fallback logic.
- **Polish the whole text at release, always**: rejected for long dictations, where the release wait grows with length.
- **Segment only on batch engines' existing VAD**: rejected. Nemotron and deepgram-stream are the defaults, and they have no VAD segments today.
