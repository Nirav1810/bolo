# 02: Snippets

**What to build:** say a trigger phrase and Bolo inserts the saved text in its place.

**Blocked by:** none (lands after 01; both touch `settings.rs`)

**Status:** done

- [x] `bolo-store`: `[[snippets]]` with `trigger` and `text`. Additive, so there's no version bump beyond 01's.
- [x] `bolo_format::snippets::expand(text, &[Snippet]) -> String` is a pure function. Matching is case-insensitive, ignores punctuation between words and is whole-word only. The longest trigger wins, and the expansion is inserted verbatim. It has golden tests, plus a proptest that text containing no trigger comes back unchanged.
- [x] The engine applies it last, to `on_final` and `on_polished` text, including the Markdown and HTML variants (HTML-escaped, line breaks as `<br>`). Snippets come in through `EngineConfig`, like the dictionary, and polish is told to leave trigger phrases as spoken.
- [x] The Dictionary tab becomes "Dictionary & snippets", with a snippet list for add, edit and remove. Checked visually in headless Edge.
- [x] Clippy, tests and `npm run check` are clean.

## Outcome

- `bolo_format::snippets`: `Snippet`, `expand`, `expand_html`, and `Snippets` (compiled once, also gives `triggers_in` for polish). Matching works on normalised words: lower case, number words zero to twenty equal their digits, anything that isn't a letter or digit between words is ignored, and a trigger never spans a line break (or, in HTML, a tag). Whole-word means a space separates the match from its neighbours, so "sig" doesn't match in "sig.example.com". Each trigger is also matched as Layer 1 writes it said alone, so "room twenty five" still matches "room 25". Overlaps go to the longest trigger. Expansions are never re-scanned.
- Punctuation: a dictation that is only the trigger becomes exactly the expansion (Layer 1's capital and period dropped; HTML keeps its `<p>`). A `.`, `?` or `!` right after a trigger is dropped when the expansion already ends in one. Otherwise the surrounding punctuation stays.
- Tests: 25 golden cases in `crates/bolo-format/tests/snippets/cases.toml` (plain, after Layer 1 in plain/Markdown/code/HTML, Hinglish, HTML escaping), plus two proptests (no trigger → unchanged, in raw, Layer 1 and HTML text; a lone trigger after Layer 1 → its expansion). There are four engine tests in `crates/bolo-core/tests/snippets.rs`.
- Engine: `EngineConfig::snippets` (serde default) and `Engine::set_snippets`, which keeps the model loaded. `on_final` is expanded, and polish gets the text before expansion. `CleanupRequest::triggers` lists only the triggers found in that part. They go in the user message as `Keep word for word: "…"`, with one static rule line in the system prompt, so the prompt stays cacheable. `on_polished` is expanded after the parts are joined.
- Settings: `SnippetRow` keeps unknown keys. `Settings::snippet_list` trims the triggers, drops blank rows and repeated triggers, and keeps the text verbatim. The desktop `Form` carries `snippets`, and a change sends `set_snippets` without reloading the model.
- UI: the `Snippets.svelte` section sits under Words on the renamed tab. It has one inline editor (trigger plus a multi-line text box, Ctrl+Enter saves, Esc cancels), flags duplicate triggers, and has edit and remove on each row. Screenshots are `snippets-*.png` in the session scratchpad.
- Known limits: a trigger that Layer 1 mangles only in context (a self-correction marker inside it, or list markers such as "first") may not match. A multi-line expansion makes the polish swap follow the existing multiline-safe rule (ADR-0018).
