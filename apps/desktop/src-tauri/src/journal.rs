//! Recording dictations in history (`bolo-store`): a row when the instant text goes in,
//! updated with the polished text and what the swap did when polish arrives.
//!
//! History failures are logged and never get in the way of a dictation. With history
//! off, or without a database, nothing is written.

use std::sync::{Mutex, MutexGuard, PoisonError};

use bolo_core::{FormattedOutput, LanguageMode, OutputFormat, Usage};
use bolo_store::{Dictation, History, UsageEvent, provider_of};

#[derive(Debug, Default)]
struct Current {
    /// The target app's exe.
    app: String,
    format: OutputFormat,
    /// The row for this dictation's instant text, once written.
    row: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Journal {
    history: Option<Mutex<History>>,
    current: Mutex<Current>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Journal {
    /// Records into `history`, or nowhere when it's `None` (the database didn't open).
    pub fn new(history: Option<History>) -> Self {
        Self { history: history.map(Mutex::new), current: Mutex::default() }
    }

    /// Runs `f` on the database, if there is one.
    pub fn with_history<R>(&self, f: impl FnOnce(&mut History) -> R) -> Option<R> {
        self.history.as_ref().map(|h| f(&mut lock(h)))
    }

    /// A dictation started into `app`, whose text goes in as `format`.
    pub fn begin(&self, app: &str, format: OutputFormat) {
        *lock(&self.current) = Current { app: app.to_owned(), format, row: None };
    }

    /// The instant text went in (or was left on the clipboard): write its row.
    pub fn finished(&self, out: &FormattedOutput) {
        let mut cur = lock(&self.current);
        cur.row = None;
        let text = if cur.format == OutputFormat::Markdown { &out.markdown } else { &out.plain };
        if text.trim().is_empty() {
            return;
        }
        let mut d = Dictation::now(&cur.app, language(out.language), &out.engine_id, text);
        d.timings = serde_json::to_value(out.timings).unwrap_or_default();
        match self.with_history(|h| h.insert(&d)) {
            Some(Ok(row)) => cur.row = row,
            Some(Err(e)) => tracing::warn!(%e, "could not record the dictation in history"),
            None => {}
        }
    }

    /// Polish arrived for the last instant text; `swap` is what the swap did.
    pub fn polished(&self, out: &FormattedOutput, swap: &str) {
        let Some(row) = lock(&self.current).row else { return };
        if let Some(Err(e)) = self.with_history(|h| h.record_polish(row, Some(&out.plain), swap)) {
            tracing::warn!(%e, "could not record the polish in history");
        }
    }
}

impl Journal {
    /// A paid cloud request finished: add it to the usage meter. Recorded with history
    /// off too, since it holds no text.
    pub fn usage(&self, usage: &Usage) {
        if let Some(Err(e)) = self.with_history(|h| h.record_usage(&usage_event(usage, bolo_store::history::now_ms())))
        {
            tracing::warn!(%e, "could not record usage");
        }
    }
}

/// The meter's row for `usage` at `at_ms`.
pub fn usage_event(usage: &Usage, at_ms: i64) -> UsageEvent {
    match usage {
        Usage::Speech { engine_id, audio_secs, cost_usd } => UsageEvent {
            at_ms,
            provider: provider_of(engine_id).to_owned(),
            detail: engine_id.clone(),
            audio_secs: *audio_secs,
            cost_usd: *cost_usd,
            ..Default::default()
        },
        Usage::Polish { model, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd } => {
            UsageEvent {
                at_ms,
                provider: bolo_core::ANTHROPIC.to_owned(),
                detail: model.clone(),
                input_tokens: *input_tokens,
                output_tokens: *output_tokens,
                cache_read_tokens: *cache_read_tokens,
                cache_write_tokens: *cache_write_tokens,
                cost_usd: *cost_usd,
                ..Default::default()
            }
        }
    }
}

fn language(l: LanguageMode) -> &'static str {
    match l {
        LanguageMode::Auto => "auto",
        LanguageMode::English => "english",
        LanguageMode::Hinglish => "hinglish",
    }
}

#[cfg(test)]
mod tests {
    use bolo_core::{FormattedOutput, LanguageMode, OutputFormat, Timings};
    use bolo_store::History;

    use super::Journal;

    fn output(plain: &str, markdown: &str) -> FormattedOutput {
        FormattedOutput {
            plain: plain.into(),
            markdown: markdown.into(),
            html: None,
            timings: Timings { stt_ms: 90.0, total_ms: 120.0, ..Default::default() },
            engine_id: "local:nemotron-stream".into(),
            language: LanguageMode::Hinglish,
        }
    }

    fn rows(j: &Journal) -> Vec<bolo_store::Dictation> {
        j.with_history(|h| h.latest(10).unwrap()).unwrap().into_iter().map(|r| r.dictation).collect()
    }

    #[test]
    fn the_final_row_is_written_then_updated_with_polish() {
        let j = Journal::new(Some(History::open_in_memory().unwrap()));
        j.begin("slack.exe", OutputFormat::Markdown);
        j.finished(&output("1. milk", "1. milk\n"));
        let [d] = rows(&j).try_into().unwrap();
        assert_eq!((d.app.as_str(), d.language.as_str()), ("slack.exe", "hinglish"));
        assert_eq!((d.engine.as_str(), d.final_text.as_str()), ("local:nemotron-stream", "1. milk\n"));
        assert_eq!(d.timings["total_ms"], 120.0);
        assert_eq!((d.polished_text, d.swap), (None, None));
        j.polished(&output("1. Milk", "1. Milk"), "swapped");
        let [d] = rows(&j).try_into().unwrap();
        assert_eq!((d.polished_text.as_deref(), d.swap.as_deref()), (Some("1. Milk"), Some("swapped")));
    }

    #[test]
    fn plain_and_html_targets_record_the_plain_text() {
        let j = Journal::new(Some(History::open_in_memory().unwrap()));
        j.begin("winword.exe", OutputFormat::Html);
        j.finished(&output("Hello.", "**Hello.**"));
        assert_eq!(rows(&j)[0].final_text, "Hello.");
    }

    #[test]
    fn polish_for_a_dictation_without_a_row_is_dropped() {
        let j = Journal::new(Some(History::open_in_memory().unwrap()));
        j.begin("notepad.exe", OutputFormat::Plain);
        j.finished(&output("  ", "  "));
        j.polished(&output("Hi.", "Hi."), "swapped");
        j.begin("notepad.exe", OutputFormat::Plain);
        j.polished(&output("Hi.", "Hi."), "not-pending");
        assert!(rows(&j).is_empty());
    }

    #[test]
    fn nothing_is_written_with_history_off() {
        let j = Journal::new(Some(History::open_in_memory().unwrap()));
        j.with_history(|h| h.set_enabled(false));
        j.begin("notepad.exe", OutputFormat::Plain);
        j.finished(&output("Secret.", "Secret."));
        j.polished(&output("Secret!", "Secret!"), "swapped");
        assert!(rows(&j).is_empty());
    }

    #[test]
    fn without_a_database_dictations_still_go_through() {
        let j = Journal::new(None);
        j.begin("notepad.exe", OutputFormat::Plain);
        j.finished(&output("Hi.", "Hi."));
        j.polished(&output("Hi!", "Hi!"), "swapped");
        assert!(j.with_history(|_| ()).is_none());
    }

    #[test]
    fn usage_is_metered_with_history_off() {
        use bolo_core::Usage;
        let mut history = History::open_in_memory().unwrap();
        history.set_enabled(false);
        let j = Journal::new(Some(history));
        j.usage(&Usage::Speech { engine_id: "deepgram-stream:nova-3".into(), audio_secs: 3.0, cost_usd: 0.0005 });
        j.usage(&Usage::Polish {
            model: "claude-haiku-5-5".into(),
            input_tokens: 90,
            output_tokens: 12,
            cache_read_tokens: 700,
            cache_write_tokens: 0,
            cost_usd: 0.00002,
        });
        let totals = j.with_history(|h| h.usage_since(0).unwrap()).unwrap();
        let providers: Vec<_> = totals.iter().map(|p| (p.provider.as_str(), p.requests)).collect();
        assert_eq!(providers, [("anthropic", 1), ("deepgram", 1)]);
        assert_eq!((totals[0].input_tokens, totals[0].cache_read_tokens), (90, 700));
        assert_eq!(totals[1].audio_secs, 3.0);
    }
}
