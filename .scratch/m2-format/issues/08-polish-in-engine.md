# 08: Polish moves into the engine

**What to build:** Polish moves from the CLI into `bolo-core` (ADR-0016). The engine emits `on_final` (the instant text), then `on_polished` when polish succeeds within its deadline.

**Blocked by:** none

**Status:** done

- [x] The engine config gains `polish: Off | On { tone, app, dictionary }`, with `Off` as the default. `On` without an Anthropic key in `ApiKeys` is a config error.
- [x] `EngineEvents::on_polished(FormattedOutput)` is added, with `timings.llm_ms` filled.
- [x] The 1.5 s deadline, the meta-reply rule and the length-ratio guard run in the engine. On timeout, error or guard failure the engine sends no `on_polished`.
- [x] The LLM client sits behind a seam, so engine tests use a fake. CI makes no network calls.
- [x] Engine tests:
  - `on_final` comes before `on_polished`.
  - Off means no polish.
  - Timeout or guard failure means no `on_polished`.
  - Cancel during polish means no `on_polished`.
- [x] `bolo dictate --polish` uses the event, and its output is unchanged for the user.

## Outcome

- `bolo-core` owns polish (`src/polish.rs`): `EngineConfig::polish` (`Polish::Off` by default, `Polish::On { tone, app, dictionary }`), the `LlmClient` seam with Claude behind it (`bolo_format::llm::cleanup`), `POLISH_DEADLINE` = 1.5 s, and the guard (`bolo_format::llm::looks_like_reply`, now public, plus an empty-answer check). The meta-reply rule stays in the cached system prompt.
- `on_final` goes out first and the dictation returns to Idle. The polish call then runs on the engine's runtime, so it never holds up the instant path or the next dictation. `on_polished` carries the polished text in both `plain` and `markdown`, `timings.llm_ms` set to the call and `total_ms` to stop → polished. The target's rendering (Markdown for Markdown targets) is what gets polished.
- No `on_polished` on a timeout, an error, a failed guard, `cancel_dictation`, or the next `start_dictation` (a new dictation supersedes the last one's polish). A generation counter, held while reporting, keeps a cancelled polish from reporting late. Why polish was skipped is logged as a `warn`.
- Polish on with no `anthropic` key fails `Engine::new` with `BoloError::Config`, and `start_dictation` does the same after an `update_config` that turned it on.
- Tests: `crates/bolo-core/tests/polish.rs` on the fake STT plus a `FakeLlm` (`Engine::with_llm_client`), with no network. They cover the order, off, a timeout, an error, the guard, cancel, a superseding dictation and the config error.
- `bolo dictate --polish` sets `Polish::On` and waits for the event. Its output lines are the same, but the deadline is now the engine's 1.5 s instead of the CLI's old 3 s, so a slow Claude call (0.8–1.3 s to the first byte, DECISIONS.md) is dropped more often. The skip line no longer names the reason; the engine's warning on stderr does. Still owed: a live `bolo dictate --polish` try by the user (mic + key).
- `bolo run` and `bolo replay --polish` keep their own bench polisher (10 s timeout, cost tally).
