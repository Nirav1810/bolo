# 02: Snippets

**What to build:** say a trigger phrase and Bolo inserts the saved text in its place.

**Blocked by:** none (lands after 01; both touch `settings.rs`)

**Status:** ready-for-agent

- [ ] `bolo-store`: `[[snippets]]` with `trigger` and `text`. Additive, so there's no version bump beyond 01's.
- [ ] `bolo_format::snippets::expand(text, &[Snippet]) -> String` is a pure function. Matching is case-insensitive, ignores punctuation between words and is whole-word only. The longest trigger wins, and the expansion is inserted verbatim. It has golden tests, plus a proptest that text containing no trigger comes back unchanged.
- [ ] The engine applies it last, to `on_final` and `on_polished` text, including the Markdown and HTML variants (HTML-escaped, line breaks as `<br>`). Snippets come in through `EngineConfig`, like the dictionary, and polish is told to leave trigger phrases as spoken.
- [ ] The Dictionary tab becomes "Dictionary & snippets", with a snippet list for add, edit and remove. Checked visually in headless Edge.
- [ ] Clippy, tests and `npm run check` are clean.
