//! Insertion and the polish swap (ADR-0018).
//!
//! **Insert.** The instant text is pasted where the cursor is: Bolo saves the user's
//! clipboard, writes the text (plus `CF_HTML` for HTML targets), sends the paste chord,
//! and puts the user's clipboard back [`RESTORE_AFTER`] later, unless the user copied
//! something in between. If the window in front runs elevated, the keys can't reach it,
//! so the text stays on the clipboard for the user to paste.
//!
//! **Swap.** When polish is on, the insert is remembered as a pending swap and the
//! keyboard and mouse hooks watch for the user's own input. When `on_polished` arrives,
//! [`swap_decision`] says whether it's still safe to select the instant text back
//! (Shift+Left once per [`caret_units`]) and paste the polished text over it.
//!
//! The [`Inserter`] does this against a [`Desktop`] and a [`Timer`], so tests run it with
//! fakes and a hand-cranked clock.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use bolo_core::{FormattedOutput, OutputFormat};
use unicode_segmentation::UnicodeSegmentation;

use crate::desktop::{Clipboard, Desktop, DesktopError, Foreground, Key};
use crate::hotkey::Flags;

/// How long the pasted text sits on the clipboard before the user's is put back. Apps
/// read the clipboard when they handle the paste, a little after the keys arrive.
pub const RESTORE_AFTER: Duration = Duration::from_millis(300);

/// Polish that arrives later than this after the insert is not swapped in.
pub const SWAP_WINDOW: Duration = Duration::from_secs(5);

/// The keys that paste in the target app. App rules choose it (ticket 07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chord {
    #[default]
    CtrlV,
    ShiftInsert,
}

impl Chord {
    fn key(self) -> Key {
        match self {
            Chord::CtrlV => Key::Paste,
            Chord::ShiftInsert => Key::ShiftInsert,
        }
    }
}

/// How a dictation's text goes into its target app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Target {
    pub format: OutputFormat,
    pub chord: Chord,
    /// The app counts a pasted line break as one Shift+Left, so multi-line text can be
    /// swapped too.
    pub multiline_swap_safe: bool,
}

/// Why polished text was not swapped in. The instant text stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The user pressed a key or clicked since the insert.
    Touched,
    /// Another window is in front.
    OtherWindow,
    /// Polish came more than [`SWAP_WINDOW`] after the insert.
    TooLate,
    /// The text has line breaks and the app isn't multi-line-safe.
    Multiline,
    /// Nothing is waiting for polish: no insert, or it was superseded, cancelled or expired.
    NotPending,
    /// The polished text is the instant text.
    Unchanged,
    /// The swap's keys were blocked.
    Blocked,
}

impl Skip {
    pub fn as_str(self) -> &'static str {
        match self {
            Skip::Touched => "touched",
            Skip::OtherWindow => "other-window",
            Skip::TooLate => "too-late",
            Skip::Multiline => "multiline",
            Skip::NotPending => "not-pending",
            Skip::Unchanged => "unchanged",
            Skip::Blocked => "blocked",
        }
    }
}

/// What to do with polished text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swap {
    Swap,
    Skip(Skip),
}

/// Whether polished text may replace the instant text (ADR-0018).
pub fn swap_decision(
    touched: bool,
    same_window: bool,
    age: Duration,
    multiline: bool,
    app_multiline_safe: bool,
) -> Swap {
    if touched {
        Swap::Skip(Skip::Touched)
    } else if !same_window {
        Swap::Skip(Skip::OtherWindow)
    } else if age > SWAP_WINDOW {
        Swap::Skip(Skip::TooLate)
    } else if multiline && !app_multiline_safe {
        Swap::Skip(Skip::Multiline)
    } else {
        Swap::Swap
    }
}

/// How many Shift+Left presses select `text` back: one per grapheme cluster, and
/// `\r\n` is a single cluster.
pub fn caret_units(text: &str) -> usize {
    text.graphemes(true).count()
}

/// Line breaks as Windows apps expect them on the clipboard: `\r\n`.
pub fn windows_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.matches('\n').count());
    let mut prev = '\0';
    for c in text.chars() {
        if c == '\n' && prev != '\r' {
            out.push('\r');
        }
        out.push(c);
        prev = c;
    }
    out
}

/// `CF_HTML` for an HTML fragment: the header with UTF-8 byte offsets of the document
/// and the fragment, then the document.
pub fn cf_html(fragment: &str) -> String {
    const HEADER: &str = "Version:0.9\r\nStartHTML:{sh}\r\nEndHTML:{eh}\r\nStartFragment:{sf}\r\nEndFragment:{ef}\r\n";
    const BEFORE: &str = "<html><body>\r\n<!--StartFragment-->";
    const AFTER: &str = "<!--EndFragment-->\r\n</body></html>";
    // Each `{..}` placeholder becomes ten digits, so the header length is known up front.
    let header_len = HEADER.len() + 4 * (10 - "{sh}".len());
    let start_fragment = header_len + BEFORE.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + AFTER.len();
    let header = HEADER
        .replace("{sh}", &format!("{header_len:010}"))
        .replace("{eh}", &format!("{end_html:010}"))
        .replace("{sf}", &format!("{start_fragment:010}"))
        .replace("{ef}", &format!("{end_fragment:010}"));
    format!("{header}{BEFORE}{fragment}{AFTER}")
}

/// What goes on the clipboard for a dictation's output.
fn clipboard_for(out: &FormattedOutput, format: OutputFormat) -> Clipboard {
    let text = match format {
        OutputFormat::Markdown => &out.markdown,
        OutputFormat::Plain | OutputFormat::Html => &out.plain,
    };
    let html = (format == OutputFormat::Html).then(|| out.html.as_deref().map(cf_html)).flatten();
    Clipboard { text: Some(windows_newlines(text)), html, ..Default::default() }
}

/// Runs a job later. [`ThreadTimer`] is the real one.
pub trait Timer: Send + Sync {
    fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>);
}

/// Each job sleeps on a thread of its own: there are at most a couple per dictation.
#[derive(Debug, Default)]
pub struct ThreadTimer;

impl Timer for ThreadTimer {
    fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>) {
        let spawned = std::thread::Builder::new().name("bolo-timer".into()).spawn(move || {
            std::thread::sleep(delay);
            job();
        });
        if let Err(e) = spawned {
            tracing::warn!(%e, "could not start a timer");
        }
    }
}

/// What an insert did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inserted {
    Pasted,
    /// The text was empty; nothing was pasted.
    Empty,
    /// The keys were blocked (an elevated window). The text is left on the clipboard.
    Blocked,
    Failed(DesktopError),
}

/// What polished text did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Swapped {
    Swapped,
    Skipped(Skip),
    Failed(DesktopError),
}

/// The user's clipboard while Bolo's text sits on it.
#[derive(Debug)]
struct Saved {
    content: Clipboard,
    /// The sequence number of Bolo's write. Anything else means the user copied since.
    ours: u32,
}

/// An insert waiting for its polish.
#[derive(Debug)]
struct Pending {
    id: u64,
    window: isize,
    at: Instant,
    text: String,
}

#[derive(Debug, Default)]
struct State {
    target: Target,
    expect_polish: bool,
    /// When the push-to-talk key was released, for the latency log.
    released: Option<Instant>,
    saved: Option<Saved>,
    /// Bumped on every paste; a restore for an older one does nothing.
    generation: u64,
    /// The id of the last pending swap; an expiry for an older one does nothing.
    swaps: u64,
    pending: Option<Pending>,
}

/// Pastes dictations and swaps in polish; see the module docs.
pub struct Inserter {
    desktop: Arc<dyn Desktop>,
    timer: Arc<dyn Timer>,
    flags: Arc<Flags>,
    /// Called when [`Flags::watching`] changes, so the hook adds or removes the mouse hook.
    watch_changed: Box<dyn Fn() + Send + Sync>,
    state: Mutex<State>,
}

impl Inserter {
    pub fn new(
        desktop: Arc<dyn Desktop>,
        timer: Arc<dyn Timer>,
        flags: Arc<Flags>,
        watch_changed: impl Fn() + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self { desktop, timer, flags, watch_changed: Box::new(watch_changed), state: Mutex::default() })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A dictation started: how its text will go in, and whether polish will follow.
    /// Any pending swap is dropped.
    pub fn begin(&self, target: Target, expect_polish: bool) {
        let mut st = self.state();
        st.target = target;
        st.expect_polish = expect_polish;
        st.released = None;
        self.drop_pending_locked(&mut st);
    }

    /// The push-to-talk key went up at `at`.
    pub fn released(&self, at: Instant) {
        self.state().released = Some(at);
    }

    /// A new dictation or a cancel: the instant text stays, and polish for it is ignored.
    pub fn drop_pending(&self) {
        let mut st = self.state();
        self.drop_pending_locked(&mut st);
    }

    fn drop_pending_locked(&self, st: &mut State) {
        if st.pending.take().is_some() {
            self.flags.stop_watching();
            (self.watch_changed)();
        }
    }

    /// Paste a dictation's instant text into the window in front.
    pub fn insert(self: &Arc<Self>, out: &FormattedOutput, now: Instant) -> Inserted {
        let mut st = self.state();
        self.drop_pending_locked(&mut st);
        let released = st.released.take();
        let clip = clipboard_for(out, st.target.format);
        let text = clip.text.clone().unwrap_or_default();
        if text.trim().is_empty() {
            return Inserted::Empty;
        }
        let window = self.desktop.foreground().map_or(0, |f: Foreground| f.window);
        let key = st.target.chord.key();
        if let Err(e) = self.paste(&mut st, &clip, &[key]) {
            return match e {
                DesktopError::Blocked => Inserted::Blocked,
                e => Inserted::Failed(e),
            };
        }
        let key_up_to_paste_ms = released.map(|r| now.saturating_duration_since(r).as_secs_f64() * 1000.0);
        tracing::info!(key_up_to_paste_ms, total_ms = out.timings.total_ms, chars = text.chars().count(), "pasted");
        if st.expect_polish {
            st.swaps += 1;
            let id = st.swaps;
            st.pending = Some(Pending { id, window, at: now, text });
            self.flags.watch_touch();
            (self.watch_changed)();
            let me = Arc::downgrade(self);
            self.timer.after(SWAP_WINDOW, Box::new(move || Self::upgrade(&me, |i| i.expire(id))));
        }
        Inserted::Pasted
    }

    /// Swap polished text in for the pending insert, if that's still safe.
    pub fn polished(self: &Arc<Self>, out: &FormattedOutput, now: Instant) -> Swapped {
        let mut st = self.state();
        let Some(pending) = st.pending.take() else { return Swapped::Skipped(Skip::NotPending) };
        let touched = self.flags.touched();
        self.flags.stop_watching();
        (self.watch_changed)();
        let window = self.desktop.foreground().map_or(0, |f| f.window);
        let decision = swap_decision(
            touched,
            window == pending.window,
            now.saturating_duration_since(pending.at),
            pending.text.contains('\n'),
            st.target.multiline_swap_safe,
        );
        if let Swap::Skip(why) = decision {
            return Swapped::Skipped(why);
        }
        let clip = clipboard_for(out, st.target.format);
        if clip.text.as_deref() == Some(pending.text.as_str()) {
            return Swapped::Skipped(Skip::Unchanged);
        }
        let mut keys = vec![Key::ShiftLeft; caret_units(&pending.text)];
        keys.push(st.target.chord.key());
        match self.paste(&mut st, &clip, &keys) {
            Ok(()) => Swapped::Swapped,
            Err(DesktopError::Blocked) => Swapped::Skipped(Skip::Blocked),
            Err(e) => Swapped::Failed(e),
        }
    }

    /// Write `clip`, send `keys`, and schedule putting the user's clipboard back.
    fn paste(self: &Arc<Self>, st: &mut State, clip: &Clipboard, keys: &[Key]) -> Result<(), DesktopError> {
        // While an earlier paste's text is still ours on the clipboard, what the user had
        // is already saved; don't save Bolo's own text over it.
        let user = match st.saved.take() {
            Some(saved) if saved.ours == self.desktop.clipboard_sequence() => Some(saved.content),
            _ => match self.desktop.read_clipboard() {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(%e, "could not save the clipboard; it won't be restored");
                    None
                }
            },
        };
        let ours = self.desktop.write_clipboard(clip)?;
        st.generation += 1;
        if let Err(e) = self.desktop.send_keys(keys) {
            // Leave the text on the clipboard so the user can paste it themselves.
            tracing::warn!(%e, "the paste was not sent; the text is left on the clipboard");
            return Err(e);
        }
        if let Some(content) = user {
            st.saved = Some(Saved { content, ours });
            let generation = st.generation;
            let me = Arc::downgrade(self);
            self.timer.after(RESTORE_AFTER, Box::new(move || Self::upgrade(&me, |i| i.restore(generation))));
        }
        Ok(())
    }

    fn upgrade(me: &Weak<Self>, f: impl FnOnce(&Self)) {
        if let Some(me) = me.upgrade() {
            f(&me);
        }
    }

    /// Put the user's clipboard back, unless a later paste took over or the user copied.
    fn restore(&self, generation: u64) {
        let mut st = self.state();
        if st.generation != generation {
            return;
        }
        let Some(saved) = st.saved.take() else { return };
        if self.desktop.clipboard_sequence() != saved.ours {
            tracing::debug!("the clipboard changed since the paste; leaving it");
            return;
        }
        if let Err(e) = self.desktop.write_clipboard(&saved.content) {
            tracing::warn!(%e, "could not restore the clipboard");
        }
    }

    /// No polish came in time: stop watching for touches.
    fn expire(&self, id: u64) {
        let mut st = self.state();
        if st.pending.as_ref().is_some_and(|p| p.id == id) {
            tracing::debug!("no polish within the swap window");
            self.drop_pending_locked(&mut st);
        }
    }
}

#[cfg(test)]
pub mod testing {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::Timer;

    type Job = Box<dyn FnOnce() + Send>;

    /// Holds jobs until the test fires them.
    #[derive(Default)]
    pub struct FakeTimer(Mutex<Vec<(Duration, Job)>>);

    impl FakeTimer {
        /// Run (and forget) every job scheduled with this delay.
        pub fn fire(&self, delay: Duration) {
            let jobs: Vec<_> = {
                let mut all = self.0.lock().unwrap();
                let (due, rest) = std::mem::take(&mut *all).into_iter().partition(|(d, _)| *d == delay);
                *all = rest;
                due
            };
            for (_, job) in jobs {
                job();
            }
        }

        pub fn delays(&self) -> Vec<Duration> {
            self.0.lock().unwrap().iter().map(|(d, _)| *d).collect()
        }
    }

    impl Timer for FakeTimer {
        fn after(&self, delay: Duration, job: Job) {
            self.0.lock().unwrap().push((delay, job));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use bolo_core::{FormattedOutput, LanguageMode, OutputFormat, Timings};

    use super::testing::FakeTimer;
    use super::*;
    use crate::desktop::fake::{FakeDesktop, Op};

    // --- pure parts ---

    #[test]
    fn swap_decision_table() {
        let ms = Duration::from_millis;
        #[rustfmt::skip]
        let cases = [
            // touched, same window, age, multiline, app safe => decision
            (false, true,  ms(800),  false, false, Swap::Swap),
            (false, true,  ms(5000), false, false, Swap::Swap),
            (false, true,  ms(5001), false, false, Swap::Skip(Skip::TooLate)),
            (true,  true,  ms(800),  false, false, Swap::Skip(Skip::Touched)),
            (false, false, ms(800),  false, false, Swap::Skip(Skip::OtherWindow)),
            (false, true,  ms(800),  true,  false, Swap::Skip(Skip::Multiline)),
            (false, true,  ms(800),  true,  true,  Swap::Swap),
            // Touched wins over every other reason.
            (true,  false, ms(9000), true,  false, Swap::Skip(Skip::Touched)),
            (false, false, ms(9000), true,  false, Swap::Skip(Skip::OtherWindow)),
            (false, true,  ms(9000), true,  false, Swap::Skip(Skip::TooLate)),
        ];
        for (touched, same, age, multi, safe, want) in cases {
            assert_eq!(swap_decision(touched, same, age, multi, safe), want, "{touched} {same} {age:?} {multi} {safe}");
        }
    }

    #[test]
    fn caret_units_count_grapheme_clusters_and_crlf_as_one() {
        assert_eq!(caret_units(""), 0);
        assert_eq!(caret_units("Hello."), 6);
        assert_eq!(caret_units("a\r\nb"), 3);
        assert_eq!(caret_units("a\nb"), 3);
        assert_eq!(caret_units("नमस्ते"), 3, "Devanagari conjuncts are single clusters");
        assert_eq!(caret_units("👍🏽 ok"), 4, "an emoji with a skin tone is one");
        assert_eq!(caret_units("é"), 1, "e + combining acute");
    }

    #[test]
    fn newlines_become_crlf_once() {
        assert_eq!(windows_newlines("a\nb\r\nc\n"), "a\r\nb\r\nc\r\n");
        assert_eq!(windows_newlines("plain"), "plain");
    }

    #[test]
    fn cf_html_offsets_point_at_the_document_and_fragment() {
        for fragment in ["<p>Hi</p>", "<ol><li>चाय</li><li>naan &amp; dal</li></ol>", ""] {
            let doc = cf_html(fragment);
            let offset = |name: &str| -> usize {
                let line = doc.lines().find(|l| l.starts_with(name)).unwrap();
                let digits = &line[name.len() + 1..];
                assert_eq!(digits.len(), 10);
                digits.parse().unwrap()
            };
            let (sh, eh, sf, ef) =
                (offset("StartHTML"), offset("EndHTML"), offset("StartFragment"), offset("EndFragment"));
            assert_eq!(&doc[sf..ef], fragment);
            assert!(doc[sh..].starts_with("<html>"));
            assert_eq!(eh, doc.len());
            assert!(doc[..sf].ends_with("<!--StartFragment-->"));
            assert!(doc[ef..].starts_with("<!--EndFragment-->"));
        }
    }

    // --- the inserter against the fake desktop ---

    fn output(plain: &str) -> FormattedOutput {
        FormattedOutput {
            plain: plain.into(),
            markdown: plain.replace("1. ", "1) "),
            html: Some(format!("<p>{plain}</p>")),
            timings: Timings { total_ms: 120.0, ..Default::default() },
            engine_id: "local:nemotron-stream".into(),
            language: LanguageMode::English,
        }
    }

    fn text(t: &str) -> Clipboard {
        Clipboard { text: Some(t.into()), ..Default::default() }
    }

    struct Rig {
        desktop: Arc<FakeDesktop>,
        timer: Arc<FakeTimer>,
        flags: Arc<Flags>,
        watch_calls: Arc<AtomicUsize>,
        inserter: Arc<Inserter>,
        t0: Instant,
    }

    impl Rig {
        fn new() -> Self {
            let desktop = Arc::new(FakeDesktop::in_front("notepad.exe", "Untitled - Notepad"));
            desktop.user_copies("the user's clipboard");
            let timer = Arc::new(FakeTimer::default());
            let flags = Arc::new(Flags::default());
            let watch_calls = Arc::new(AtomicUsize::new(0));
            let calls = watch_calls.clone();
            let inserter = Inserter::new(desktop.clone(), timer.clone(), flags.clone(), move || {
                calls.fetch_add(1, Ordering::Relaxed);
            });
            Self { desktop, timer, flags, watch_calls, inserter, t0: Instant::now() }
        }

        fn at(&self, ms: u64) -> Instant {
            self.t0 + Duration::from_millis(ms)
        }

        /// Begin a polished dictation, insert `instant` at 0 ms and clear the log.
        fn inserted(&self, instant: &str) {
            self.inserter.begin(Target::default(), true);
            assert_eq!(self.inserter.insert(&output(instant), self.at(0)), Inserted::Pasted);
            self.desktop.take_ops();
        }
    }

    #[test]
    fn insert_saves_writes_pastes_then_restores() {
        let rig = Rig::new();
        rig.inserter.begin(Target::default(), false);
        assert_eq!(rig.inserter.insert(&output("Hello\nthere."), rig.at(0)), Inserted::Pasted);
        assert_eq!(
            rig.desktop.take_ops(),
            [Op::Read, Op::Write(text("Hello\r\nthere.")), Op::Keys(vec![Key::Paste])],
            "the user's clipboard is saved before Bolo writes, and the paste comes last"
        );
        assert_eq!(rig.timer.delays(), [RESTORE_AFTER], "no swap pending without polish");
        rig.timer.fire(RESTORE_AFTER);
        assert_eq!(rig.desktop.take_ops(), [Op::Write(text("the user's clipboard"))]);
    }

    #[test]
    fn the_user_copying_before_the_restore_wins() {
        let rig = Rig::new();
        rig.inserter.begin(Target::default(), false);
        rig.inserter.insert(&output("Hello."), rig.at(0));
        rig.desktop.take_ops();
        rig.desktop.user_copies("copied meanwhile");
        rig.timer.fire(RESTORE_AFTER);
        assert_eq!(rig.desktop.take_ops(), [], "no restore over the user's copy");
        assert_eq!(rig.desktop.text().as_deref(), Some("copied meanwhile"));
    }

    #[test]
    fn html_targets_get_cf_html_and_shift_insert_apps_get_their_chord() {
        let rig = Rig::new();
        let target = Target { format: OutputFormat::Html, chord: Chord::ShiftInsert, multiline_swap_safe: false };
        rig.inserter.begin(target, false);
        rig.inserter.insert(&output("Hi."), rig.at(0));
        let ops = rig.desktop.take_ops();
        let Op::Write(clip) = &ops[1] else { panic!("{ops:?}") };
        assert_eq!(clip.text.as_deref(), Some("Hi."));
        assert_eq!(clip.html.as_deref(), Some(cf_html("<p>Hi.</p>").as_str()));
        assert_eq!(ops[2], Op::Keys(vec![Key::ShiftInsert]));
    }

    #[test]
    fn markdown_targets_get_the_markdown() {
        let rig = Rig::new();
        rig.inserter.begin(Target { format: OutputFormat::Markdown, ..Default::default() }, false);
        rig.inserter.insert(&output("1. tea"), rig.at(0));
        assert_eq!(rig.desktop.take_ops()[1], Op::Write(text("1) tea")));
    }

    #[test]
    fn empty_text_pastes_nothing() {
        let rig = Rig::new();
        rig.inserter.begin(Target::default(), true);
        assert_eq!(rig.inserter.insert(&output("  "), rig.at(0)), Inserted::Empty);
        assert_eq!(rig.desktop.take_ops(), []);
        assert!(!rig.flags.watching());
    }

    #[test]
    fn a_blocked_paste_leaves_the_text_on_the_clipboard() {
        let rig = Rig::new();
        rig.desktop.set_blocked(true);
        rig.inserter.begin(Target::default(), true);
        assert_eq!(rig.inserter.insert(&output("Hello."), rig.at(0)), Inserted::Blocked);
        assert!(rig.timer.delays().is_empty(), "no restore, no swap");
        assert_eq!(rig.desktop.text().as_deref(), Some("Hello."));
        assert!(!rig.flags.watching());
    }

    #[test]
    fn polish_swaps_by_selecting_back_and_pasting() {
        let rig = Rig::new();
        rig.inserter.begin(Target::default(), true);
        rig.inserter.insert(&output("helo nam"), rig.at(0));
        assert!(rig.flags.watching(), "watching for the user's input while the swap is pending");
        assert_eq!(rig.watch_calls.load(Ordering::Relaxed), 1, "the mouse hook goes in");
        rig.desktop.take_ops();
        assert_eq!(rig.inserter.polished(&output("Hello, Naam."), rig.at(900)), Swapped::Swapped);
        let mut keys = vec![Key::ShiftLeft; 8];
        keys.push(Key::Paste);
        assert_eq!(
            rig.desktop.take_ops(),
            [Op::Write(text("Hello, Naam.")), Op::Keys(keys)],
            "the user's clipboard was saved by the insert and isn't read again over Bolo's text"
        );
        assert!(!rig.flags.watching());
        assert_eq!(rig.watch_calls.load(Ordering::Relaxed), 2, "the mouse hook comes out");
        // The insert's restore is stale now; the swap's restores the user's clipboard.
        rig.timer.fire(RESTORE_AFTER);
        assert_eq!(rig.desktop.take_ops(), [Op::Write(text("the user's clipboard"))]);
    }

    #[test]
    fn polish_after_the_restore_saves_the_clipboard_again() {
        let rig = Rig::new();
        rig.inserted("helo");
        rig.timer.fire(RESTORE_AFTER);
        rig.desktop.take_ops();
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(1200)), Swapped::Swapped);
        assert_eq!(rig.desktop.take_ops()[0], Op::Read);
    }

    #[test]
    fn a_touch_skips_the_swap() {
        let rig = Rig::new();
        rig.inserted("helo");
        rig.flags.touch();
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::Touched));
        assert_eq!(rig.desktop.take_ops(), []);
    }

    #[test]
    fn another_window_skips_the_swap() {
        let rig = Rig::new();
        rig.inserted("helo");
        rig.desktop.focus("chrome.exe", "Inbox", 2);
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::OtherWindow));
    }

    #[test]
    fn late_polish_skips_the_swap() {
        let rig = Rig::new();
        rig.inserted("helo");
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(5001)), Swapped::Skipped(Skip::TooLate));
    }

    #[test]
    fn multiline_text_swaps_only_in_safe_apps() {
        let rig = Rig::new();
        rig.inserted("one\ntwo");
        assert_eq!(rig.inserter.polished(&output("One.\nTwo."), rig.at(900)), Swapped::Skipped(Skip::Multiline));

        let target = Target { multiline_swap_safe: true, ..Default::default() };
        rig.inserter.begin(target, true);
        rig.inserter.insert(&output("one\ntwo"), rig.at(0));
        rig.desktop.take_ops();
        assert_eq!(rig.inserter.polished(&output("One.\nTwo."), rig.at(900)), Swapped::Swapped);
        let ops = rig.desktop.take_ops();
        let Op::Keys(keys) = &ops[1] else { panic!("{ops:?}") };
        assert_eq!(keys.iter().filter(|k| **k == Key::ShiftLeft).count(), 7, "\\r\\n is one caret unit");
    }

    #[test]
    fn unchanged_polish_is_not_swapped() {
        let rig = Rig::new();
        rig.inserted("Hello.");
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::Unchanged));
        assert_eq!(rig.desktop.take_ops(), []);
    }

    #[test]
    fn a_new_dictation_or_a_cancel_drops_the_pending_swap() {
        let rig = Rig::new();
        rig.inserted("helo");
        rig.inserter.begin(Target::default(), true);
        assert!(!rig.flags.watching());
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::NotPending));

        rig.inserted("helo");
        rig.inserter.drop_pending();
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::NotPending));
        assert_eq!(rig.desktop.take_ops(), []);
    }

    #[test]
    fn the_swap_window_expires_and_unhooks_the_mouse() {
        let rig = Rig::new();
        rig.inserted("helo");
        rig.timer.fire(SWAP_WINDOW);
        assert!(!rig.flags.watching());
        assert_eq!(rig.watch_calls.load(Ordering::Relaxed), 2);
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::NotPending));
    }

    #[test]
    fn an_old_expiry_leaves_a_newer_swap_alone() {
        let rig = Rig::new();
        rig.inserted("first");
        rig.inserted("second");
        rig.timer.fire(SWAP_WINDOW);
        assert_eq!(rig.inserter.polished(&output("Second."), rig.at(900)), Swapped::Skipped(Skip::NotPending));
        // Both expiries fired together here; check the pure generation rule on its own too.
        rig.inserted("third");
        let id = rig.inserter.state().pending.as_ref().unwrap().id;
        rig.inserter.expire(id - 1);
        assert!(rig.inserter.state().pending.is_some());
    }

    #[test]
    fn a_blocked_swap_is_skipped() {
        let rig = Rig::new();
        rig.inserted("helo");
        rig.desktop.set_blocked(true);
        assert_eq!(rig.inserter.polished(&output("Hello."), rig.at(900)), Swapped::Skipped(Skip::Blocked));
    }
}
