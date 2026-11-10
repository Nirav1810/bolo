# Layer 1 formatting is a fixed pipeline of conservative built-in passes

Raw recognition is not text you'd send: it has fillers, spoken self-corrections, number words and spoken lists. Layer 1 fixes these on the **instant path**, so it must be fast and must never make the text worse. It is a **fixed, ordered pipeline of hand-written Rust passes** in `bolo-format`. The order is fillers → self-corrections → numbers → lists and commands → punctuation fix-ups → render for the target format. Word lists, markers and number words are data tables inside the crate. All passes together must stay under **1 ms p95** on the 40-clip set, measured inside `rules_ms`.

The guiding rule is **when unsure, leave it verbatim**. A wrong deletion on the instant path is worse than a missed fix, because **polish** can catch a miss but nothing restores a lost word. Concretely:

- **Self-corrections** act only on explicit markers ("actually no", "scratch that", "I mean", "sorry", "nahi nahi", "nahi wait"). They replace only the nearest preceding phrase of the same kind (date, time, number, noun phrase).
- **Hindi fillers** ("matlab", "haan toh", "na", "woh") are removed only where they are fillers, such as at the start of a dictation or when repeated, because they are often real words.
- **Number words** are English only. Hindi number words ("do sau pachaas") are left for polish.
- **Lists** form only from an explicit enumeration of at least three items ("first … second … third", "one … two … three", "number one …"). Prose joined with "and" never becomes a list.
- **Spoken commands** are only "new line" and "new paragraph", plus Hinglish forms ("nayi line"). Spoken punctuation ("comma", "full stop") is not a command, because people say those words in normal speech.
- **Punctuation** gets fix-ups only: sentence-start capitals, a final period, spacing, a capital "I" and no doubled punctuation. Bolo does not use a punctuation model, and adding missing commas is polish's job.
- **Number style:**
  - Digits, except one through nine in prose.
  - Money, times, dates and percentages always use digits.
  - Grouping is Indian (`₹1,25,000`) when the speaker says lakh, crore or rupees, and Western (`125,000`) otherwise.
- **Output:** Plain and Markdown rendering. `code_mode` drops sentence capitals and the final period. HTML waits for M3.

Each pass is tested with TOML golden cases (`input` / `expected` / `note`) run by a single test, plus property tests: a pass never drops a word that isn't a filler, and running it twice gives the same result. A user-reported regression becomes one new golden case.

## Considered Options

- **User-editable rule file (regex/TOML at runtime)**: rejected for M2. Ordering and the "leave it verbatim" guarantees are hard to keep when users write rules. User replacements (a personal dictionary) arrive with the M3 settings file and run after the built-in passes.
- **A punctuation or truecasing model**: rejected. Most engines already punctuate, a model costs milliseconds, and polish covers the rest.
- **Aggressive self-correction (any "no" or "sorry")**: rejected because of wrong deletions on the instant path.
