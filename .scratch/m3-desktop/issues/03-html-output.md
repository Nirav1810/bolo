# 03: HTML output from the rules pipeline and polish

**What to build:** `bolo_format::rules::format` renders HTML next to plain and Markdown, and the engine fills `FormattedOutput::html` for `OutputFormat::Html` targets, for polished output too.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] HTML rendering from the same token run: `<p>` paragraphs (new paragraph), `<br>` for a new line, `<ol><li>` for lists. Every text node is escaped.
- [ ] The engine sets `html: Some(..)` when the target format is `Html`, in both `on_final` and `on_polished`. Polished text is parsed back through the same renderer (paragraphs, lines, numbered lists). Raw model output is never trusted as HTML.
- [ ] Golden cases in `crates/bolo-format/tests/golden/` for HTML, including escaping (`<`, `&`, quotes).
- [ ] Property test: the HTML's text content equals the plain rendering, modulo whitespace and list numbering, and the input never produces an unescaped tag.
- [ ] The latency gate (< 1 ms p95) still holds with the third rendering.
