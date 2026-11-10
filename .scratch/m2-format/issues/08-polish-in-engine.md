# 08: Polish moves into the engine

**What to build:** Polish moves from the CLI into `bolo-core` (ADR-0016). The engine emits `on_final` (the instant text), then `on_polished` when polish succeeds within its deadline.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] The engine config gains `polish: Off | On { tone, app, dictionary }`, with `Off` as the default. `On` without an Anthropic key in `ApiKeys` is a config error.
- [ ] `EngineEvents::on_polished(FormattedOutput)` is added, with `timings.llm_ms` filled.
- [ ] The 1.5 s deadline, the meta-reply rule and the length-ratio guard run in the engine. On timeout, error or guard failure the engine sends no `on_polished`.
- [ ] The LLM client sits behind a seam, so engine tests use a fake. CI makes no network calls.
- [ ] Engine tests:
  - `on_final` comes before `on_polished`.
  - Off means no polish.
  - Timeout or guard failure means no `on_polished`.
  - Cancel during polish means no `on_polished`.
- [ ] `bolo dictate --polish` uses the event, and its output is unchanged for the user.
