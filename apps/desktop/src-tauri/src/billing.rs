//! The providers' own usage reports, for the Usage page, when the user adds a key that
//! may read them. Bolo's meter (`bolo-store` usage) works with any key; these add what
//! only the provider knows: usage from other apps on the account, and credit left.
//!
//! Each report reads with the key on Engines & keys when that key may read usage, so
//! nothing is asked twice; a separate usage key is only for when it may not.
//!
//! - Deepgram: the engine key, or a key with the Admin (or Owner) role stored as
//!   `deepgram-admin`, which wins when both are there. Reads
//!   `GET /v1/projects/{id}/usage/breakdown` (hours, requests), `/billing/breakdown`
//!   (dollars) and `/balances` (credit left). An ordinary Member key gets 403.
//! - Anthropic: an organization Admin key (`sk-ant-admin01-…`), stored as
//!   `anthropic-admin` (or pasted as the engine key). An ordinary `sk-ant-api…` key can't
//!   read the Admin API at all, so it is never tried. Reads `GET /v1/organizations/usage_report/messages` for Bolo's own
//!   key (found by its hint in `/v1/organizations/api_keys`) and `/cost_report` for the
//!   whole account. Individual Console accounts have no Admin API.
//!
//! Days are UTC days, as both APIs count them. Results are cached for a few minutes:
//! the reports trail requests by about five minutes anyway.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const DEEPGRAM_ADMIN: &str = "deepgram-admin";
pub const ANTHROPIC_ADMIN: &str = "anthropic-admin";

const DEEPGRAM_API: &str = "https://api.deepgram.com/v1";
const ANTHROPIC_API: &str = "https://api.anthropic.com/v1/organizations";
const CACHE_FOR: Duration = Duration::from_secs(5 * 60);
const TIMEOUT: Duration = Duration::from_secs(15);
const MS_PER_DAY: i64 = 86_400_000;
/// How far back "all time" reaches on Anthropic, which pages 31 days at a time.
const ALL_TIME_DAYS: i64 = 366;

/// One period's numbers, for each of today (UTC), the last 30 days and all time.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Periods<T> {
    pub today: T,
    pub last_30_days: T,
    pub all_time: T,
}

/// A provider's report, or why there isn't one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Report<T> {
    /// No usage key stored.
    NoKey,
    Ok {
        report: T,
    },
    /// The provider refused the key: it can't read usage.
    Refused {
        message: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DeepgramFigures {
    pub hours: f64,
    pub requests: f64,
    /// `None` when the key can't read billing.
    pub usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeepgramReport {
    pub project: String,
    pub periods: Periods<DeepgramFigures>,
    /// Credit left, in `balance_units` (USD); `None` when the key can't read it.
    pub balance: Option<f64>,
    pub balance_units: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AnthropicFigures {
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnthropicReport {
    /// The Console name of Bolo's key, when its hint matched one; otherwise the token
    /// figures are the whole organization's.
    pub key_name: Option<String>,
    pub periods: Periods<AnthropicFigures>,
    /// What the whole account was charged (USD), from the cost report. All time here is
    /// the last 366 days.
    pub account_usd: Periods<f64>,
}

/// Which key a report read with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeySource {
    #[default]
    None,
    /// The key on Engines & keys.
    Engine,
    /// A separate usage key (`deepgram-admin`, `anthropic-admin`).
    Usage,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderReports {
    pub deepgram: Report<DeepgramReport>,
    pub anthropic: Report<AnthropicReport>,
    pub deepgram_key: KeySource,
    pub anthropic_key: KeySource,
    pub fetched_ms: i64,
}

static CACHE: Mutex<Option<(Instant, ProviderReports)>> = Mutex::new(None);

/// The keys stored: the usage keys, and the engine keys (Bolo's Anthropic key is also
/// the one to find in Anthropic's report).
#[derive(Clone, Default)]
pub struct Keys {
    pub deepgram_admin: Option<String>,
    pub anthropic_admin: Option<String>,
    pub deepgram: Option<String>,
    pub anthropic: Option<String>,
}

/// The key to read a report with: the usage key, else the engine key when `engine_can`
/// read usage.
fn pick(usage: Option<&str>, engine: Option<&str>, engine_can: fn(&str) -> bool) -> (Option<String>, KeySource) {
    let present = |k: Option<&str>| k.map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned);
    if let Some(k) = present(usage) {
        return (Some(k), KeySource::Usage);
    }
    match present(engine) {
        Some(k) if engine_can(&k) => (Some(k), KeySource::Engine),
        _ => (None, KeySource::None),
    }
}

/// Only an Admin key reads Anthropic's usage; an ordinary API key gets 401.
fn is_anthropic_admin(key: &str) -> bool {
    key.starts_with("sk-ant-admin")
}

/// The reports, from the cache when they are fresh and `refresh` is false.
pub async fn reports(keys: Keys, now_ms: i64, refresh: bool) -> ProviderReports {
    if !refresh
        && let Some((at, cached)) = crate::lock(&CACHE).as_ref()
        && at.elapsed() < CACHE_FOR
    {
        return cached.clone();
    }
    let client = match bolo_stt::cloud::http_client() {
        Ok(c) => c,
        Err(e) => {
            let message = e.to_string();
            return ProviderReports {
                deepgram: Report::Failed { message: message.clone() },
                anthropic: Report::Failed { message },
                deepgram_key: KeySource::None,
                anthropic_key: KeySource::None,
                fetched_ms: now_ms,
            };
        }
    };
    let (dg_key, deepgram_key) = pick(keys.deepgram_admin.as_deref(), keys.deepgram.as_deref(), |_| true);
    let (an_key, anthropic_key) = pick(keys.anthropic_admin.as_deref(), keys.anthropic.as_deref(), is_anthropic_admin);
    let deepgram = async {
        match &dg_key {
            None => Report::NoKey,
            Some(k) => deepgram(&client, k, now_ms).await.into(),
        }
    };
    let anthropic = async {
        match &an_key {
            None => Report::NoKey,
            Some(k) => anthropic(&client, k, keys.anthropic.as_deref(), now_ms).await.into(),
        }
    };
    let (deepgram, anthropic) = (deepgram.await, anthropic.await);
    let out = ProviderReports { deepgram, anthropic, deepgram_key, anthropic_key, fetched_ms: now_ms };
    *crate::lock(&CACHE) = Some((Instant::now(), out.clone()));
    out
}

/// Forget the cached reports (a usage key changed).
pub fn forget() {
    *crate::lock(&CACHE) = None;
}

/// Why a request failed: refused (401/403) or anything else.
#[derive(Debug)]
enum Error {
    Refused(String),
    Failed(String),
}

impl<T> From<Result<T, Error>> for Report<T> {
    fn from(r: Result<T, Error>) -> Self {
        match r {
            Ok(report) => Report::Ok { report },
            Err(Error::Refused(message)) => Report::Refused { message },
            Err(Error::Failed(message)) => Report::Failed { message },
        }
    }
}

async fn get<T: for<'de> Deserialize<'de>>(req: reqwest::RequestBuilder, what: &str) -> Result<T, Error> {
    let resp = req.timeout(TIMEOUT).send().await.map_err(|e| Error::Failed(format!("{what}: {}", e.without_url())))?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(Error::Refused(format!("{what}: the key may not read this (HTTP {status})")));
    }
    if !(200..300).contains(&status) {
        let body: String = resp.text().await.unwrap_or_default().chars().take(200).collect();
        return Err(Error::Failed(format!("{what}: HTTP {status} {body}")));
    }
    resp.json().await.map_err(|e| Error::Failed(format!("{what}: {e}")))
}

// --- Deepgram -------------------------------------------------------------------------

#[derive(Deserialize)]
struct DgProjects {
    #[serde(default)]
    projects: Vec<DgProject>,
}
#[derive(Deserialize)]
struct DgProject {
    project_id: String,
    #[serde(default)]
    name: String,
}
#[derive(Deserialize)]
struct DgUsage {
    #[serde(default)]
    results: Vec<DgUsageRow>,
}
#[derive(Deserialize)]
struct DgUsageRow {
    #[serde(default)]
    hours: f64,
    #[serde(default)]
    requests: f64,
}
#[derive(Deserialize)]
struct DgBilling {
    #[serde(default)]
    results: Vec<DgBillingRow>,
}
#[derive(Deserialize)]
struct DgBillingRow {
    #[serde(default)]
    dollars: f64,
}
#[derive(Deserialize)]
struct DgBalances {
    #[serde(default)]
    balances: Vec<DgBalance>,
}
#[derive(Deserialize)]
struct DgBalance {
    #[serde(default)]
    amount: f64,
    units: Option<String>,
}

/// Sum a breakdown's rows (one per grouping, or one in all without a grouping).
fn dg_sum(u: &DgUsage) -> (f64, f64) {
    u.results.iter().fold((0.0, 0.0), |(h, r), row| (h + row.hours, r + row.requests))
}

/// The `start`/`end` dates (inclusive) of each period, `YYYY-MM-DD` in UTC.
fn dg_ranges(now_ms: i64) -> [(String, String); 3] {
    let today = date(now_ms);
    [(today.clone(), today.clone()), (date(now_ms - 29 * MS_PER_DAY), today.clone()), ("2015-01-01".into(), today)]
}

async fn deepgram(client: &reqwest::Client, key: &str, now_ms: i64) -> Result<DeepgramReport, Error> {
    let auth = format!("Token {key}");
    let projects: DgProjects =
        get(client.get(format!("{DEEPGRAM_API}/projects")).header("authorization", &auth), "projects").await?;
    let project = projects.projects.into_iter().next().ok_or_else(|| Error::Failed("no Deepgram project".into()))?;
    let base = format!("{DEEPGRAM_API}/projects/{}", project.project_id);
    let mut figures = Vec::new();
    for (start, end) in dg_ranges(now_ms) {
        let range = [("start", start), ("end", end)];
        let usage: DgUsage =
            get(client.get(format!("{base}/usage/breakdown")).header("authorization", &auth).query(&range), "usage")
                .await?;
        let usd = get::<DgBilling>(
            client.get(format!("{base}/billing/breakdown")).header("authorization", &auth).query(&range),
            "billing",
        )
        .await
        .ok()
        .map(|b| b.results.iter().map(|r| r.dollars).sum());
        let (hours, requests) = dg_sum(&usage);
        figures.push(DeepgramFigures { hours, requests, usd });
    }
    let balances =
        get::<DgBalances>(client.get(format!("{base}/balances")).header("authorization", &auth), "balances").await.ok();
    let (balance, balance_units) = match balances {
        Some(b) if !b.balances.is_empty() => {
            (Some(b.balances.iter().map(|x| x.amount).sum()), b.balances.into_iter().find_map(|x| x.units))
        }
        _ => (None, None),
    };
    let [today, last_30_days, all_time]: [DeepgramFigures; 3] =
        figures.try_into().map_err(|_| Error::Failed("usage periods".into()))?;
    Ok(DeepgramReport {
        project: project.name,
        periods: Periods { today, last_30_days, all_time },
        balance,
        balance_units,
    })
}

// --- Anthropic ------------------------------------------------------------------------

#[derive(Deserialize)]
struct AnKeys {
    #[serde(default)]
    data: Vec<AnKey>,
    #[serde(default)]
    has_more: bool,
    last_id: Option<String>,
}
#[derive(Deserialize, Clone)]
struct AnKey {
    id: String,
    #[serde(default)]
    name: String,
    partial_key_hint: Option<String>,
}
#[derive(Deserialize)]
struct AnPage<T> {
    #[serde(default = "Vec::new")]
    data: Vec<AnBucket<T>>,
    #[serde(default)]
    has_more: bool,
    next_page: Option<String>,
}
#[derive(Deserialize)]
struct AnBucket<T> {
    starting_at: String,
    #[serde(default = "Vec::new")]
    results: Vec<T>,
}
#[derive(Deserialize, Default)]
struct AnUsage {
    #[serde(default)]
    uncached_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation: AnCacheCreation,
    #[serde(default)]
    output_tokens: u64,
}
#[derive(Deserialize, Default)]
struct AnCacheCreation {
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
}
#[derive(Deserialize)]
struct AnCost {
    /// Cents, as a decimal string.
    amount: String,
}

/// The key whose hint ends like `key` (hints look like `sk-ant-api03-R2D...igAA`).
fn match_key<'a>(keys: &'a [AnKey], key: &str) -> Option<&'a AnKey> {
    let key = key.trim();
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let head: String = key.chars().take(16).collect();
    let mut found = keys.iter().filter(|k| {
        k.partial_key_hint.as_deref().is_some_and(|h| {
            let (start, end) = h.split_once("...").unwrap_or(("", h));
            end == tail && head.starts_with(start)
        })
    });
    let first = found.next()?;
    // Two keys that look alike: don't guess.
    found.next().is_none().then_some(first)
}

/// Add each daily bucket at or after `today` to `today`, after `month_start` to the
/// 30 days, and all of them to all time.
fn add_usage(buckets: &[AnBucket<AnUsage>], today: &str, month_start: &str, p: &mut Periods<AnthropicFigures>) {
    for b in buckets {
        let day = &b.starting_at[..b.starting_at.len().min(10)];
        for r in &b.results {
            let f = AnthropicFigures {
                input_tokens: r.uncached_input_tokens,
                cache_read_tokens: r.cache_read_input_tokens,
                cache_write_tokens: r.cache_creation.ephemeral_1h_input_tokens
                    + r.cache_creation.ephemeral_5m_input_tokens,
                output_tokens: r.output_tokens,
            };
            if day >= today {
                plus(&mut p.today, &f);
            }
            if day >= month_start {
                plus(&mut p.last_30_days, &f);
            }
            plus(&mut p.all_time, &f);
        }
    }
}

fn plus(a: &mut AnthropicFigures, b: &AnthropicFigures) {
    a.input_tokens += b.input_tokens;
    a.cache_read_tokens += b.cache_read_tokens;
    a.cache_write_tokens += b.cache_write_tokens;
    a.output_tokens += b.output_tokens;
}

/// USD from the cost report's daily buckets (amounts are cents).
fn add_cost(buckets: &[AnBucket<AnCost>], today: &str, month_start: &str, p: &mut Periods<f64>) {
    for b in buckets {
        let day = &b.starting_at[..b.starting_at.len().min(10)];
        let usd: f64 = b.results.iter().filter_map(|r| r.amount.parse::<f64>().ok()).sum::<f64>() / 100.0;
        if day >= today {
            p.today += usd;
        }
        if day >= month_start {
            p.last_30_days += usd;
        }
        p.all_time += usd;
    }
}

/// Every daily bucket of `path` from `start` on, page by page (31 days a page).
async fn an_buckets<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    start: &str,
    extra: &[(&str, String)],
) -> Result<Vec<AnBucket<T>>, Error> {
    let mut out = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..20 {
        let mut q: Vec<(&str, String)> =
            vec![("starting_at", format!("{start}T00:00:00Z")), ("bucket_width", "1d".into()), ("limit", "31".into())];
        q.extend(extra.iter().cloned());
        if let Some(p) = &page {
            q.push(("page", p.clone()));
        }
        let req = client
            .get(format!("{ANTHROPIC_API}/{path}"))
            .header("x-api-key", key)
            .header("anthropic-version", bolo_format::llm::API_VERSION)
            .query(&q);
        let p: AnPage<T> = get(req, path).await?;
        out.extend(p.data);
        match (p.has_more, p.next_page) {
            (true, Some(next)) => page = Some(next),
            _ => break,
        }
    }
    Ok(out)
}

async fn anthropic(
    client: &reqwest::Client,
    admin: &str,
    bolo_key: Option<&str>,
    now_ms: i64,
) -> Result<AnthropicReport, Error> {
    let today = date(now_ms);
    let month_start = date(now_ms - 29 * MS_PER_DAY);
    let year_start = date(now_ms - (ALL_TIME_DAYS - 1) * MS_PER_DAY);

    // Bolo's key, to report its usage alone.
    let mut keys = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let mut q = vec![("limit", "1000".to_owned())];
        if let Some(a) = &after {
            q.push(("after_id", a.clone()));
        }
        let req = client
            .get(format!("{ANTHROPIC_API}/api_keys"))
            .header("x-api-key", admin)
            .header("anthropic-version", bolo_format::llm::API_VERSION)
            .query(&q);
        let page: AnKeys = get(req, "API keys").await?;
        keys.extend(page.data);
        match (page.has_more, page.last_id) {
            (true, Some(last)) => after = Some(last),
            _ => break,
        }
    }
    let bolo = bolo_key.and_then(|k| match_key(&keys, k)).cloned();
    let filter: Vec<(&str, String)> = bolo.iter().map(|k| ("api_key_ids[]", k.id.clone())).collect();

    let usage: Vec<AnBucket<AnUsage>> =
        an_buckets(client, admin, "usage_report/messages", &year_start, &filter).await?;
    let mut periods = Periods::default();
    add_usage(&usage, &today, &month_start, &mut periods);

    let cost: Vec<AnBucket<AnCost>> = an_buckets(client, admin, "cost_report", &year_start, &[]).await?;
    let mut account_usd = Periods::default();
    add_cost(&cost, &today, &month_start, &mut account_usd);

    Ok(AnthropicReport { key_name: bolo.map(|k| k.name), periods, account_usd })
}

// --- Dates ----------------------------------------------------------------------------

/// `YYYY-MM-DD` (UTC) of a Unix time in milliseconds.
pub fn date(ms: i64) -> String {
    let (y, m, d) = civil(ms.div_euclid(MS_PER_DAY));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Year, month, day of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_read_with_the_engine_key_when_it_can() {
        let any = |_: &str| true;
        assert_eq!(pick(None, Some("dg"), any), (Some("dg".into()), KeySource::Engine));
        assert_eq!(pick(Some("admin"), Some("dg"), any), (Some("admin".into()), KeySource::Usage));
        assert_eq!(pick(Some(" "), Some("dg"), any), (Some("dg".into()), KeySource::Engine));
        assert_eq!(pick(None, None, any), (None, KeySource::None));
        // An ordinary Anthropic key can't read usage: not tried.
        assert_eq!(pick(None, Some("sk-ant-api03-x"), is_anthropic_admin), (None, KeySource::None));
        assert_eq!(
            pick(None, Some("sk-ant-admin01-x"), is_anthropic_admin),
            (Some("sk-ant-admin01-x".into()), KeySource::Engine)
        );
    }

    #[test]
    fn dates_are_utc_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400_000), "2000-02-29");
        // 2026-11-11T23:59:59Z, then the next day.
        assert_eq!(date(1_794_441_599_000), "2026-11-11");
        assert_eq!(date(1_794_441_600_000), "2026-11-12");
        let [today, month, all] = dg_ranges(1_794_441_599_000);
        assert_eq!(today, ("2026-11-11".into(), "2026-11-11".into()));
        assert_eq!(month.0, "2026-10-13");
        assert_eq!(all.1, "2026-11-11");
    }

    #[test]
    fn deepgram_breakdowns_add_up() {
        let u: DgUsage = serde_json::from_str(
            r#"{"start":"2026-10-13","end":"2026-11-11","resolution":{"units":"day","amount":1},
                "results":[{"hours":1.5,"total_hours":1.5,"agent_hours":0,"tokens_in":0,"tokens_out":0,
                            "tts_characters":0,"requests":30,"grouping":{}},
                           {"hours":0.25,"requests":4}]}"#,
        )
        .unwrap();
        assert_eq!(dg_sum(&u), (1.75, 34.0));
        let b: DgBalances =
            serde_json::from_str(r#"{"balances":[{"balance_id":"b","amount":187.42,"units":"usd"}]}"#).unwrap();
        assert_eq!(b.balances[0].amount, 187.42);
    }

    fn key(id: &str, hint: &str) -> AnKey {
        AnKey { id: id.into(), name: id.into(), partial_key_hint: Some(hint.into()) }
    }

    #[test]
    fn bolos_anthropic_key_is_found_by_its_hint() {
        let keys = [key("apikey_a", "sk-ant-api03-R2D...igAA"), key("apikey_b", "sk-ant-api03-Xy1...zzzz")];
        let found = |k: &str| match_key(&keys, k).map(|k| k.id.as_str());
        assert_eq!(found("sk-ant-api03-R2Dsecretmiddle-igAA"), Some("apikey_a"));
        assert_eq!(found("sk-ant-api03-Qqqsecret-igAA"), None, "same tail, other start");
        assert_eq!(found("sk-ant-api03-R2Dsecret-nope"), None);
        let twins = [key("a", "sk-ant-api03-R2D...igAA"), key("b", "sk-ant-api03-R2D...igAA")];
        assert!(match_key(&twins, "sk-ant-api03-R2Dx-igAA").is_none(), "ambiguous: no guess");
    }

    #[test]
    fn anthropic_reports_sum_into_periods() {
        let usage: AnPage<AnUsage> = serde_json::from_str(
            r#"{"data":[
                {"starting_at":"2026-09-01T00:00:00Z","ending_at":"2026-09-02T00:00:00Z","results":[
                    {"uncached_input_tokens":100,"cache_read_input_tokens":1000,
                     "cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":50},
                     "output_tokens":20,"model":"claude-haiku-5-5","api_key_id":"apikey_a"}]},
                {"starting_at":"2026-11-11T00:00:00Z","ending_at":"2026-11-12T00:00:00Z","results":[
                    {"uncached_input_tokens":10,"cache_read_input_tokens":700,"output_tokens":5}]},
                {"starting_at":"2026-11-10T00:00:00Z","ending_at":"2026-11-11T00:00:00Z","results":[]}],
              "has_more":false,"next_page":null}"#,
        )
        .unwrap();
        let mut p = Periods::default();
        add_usage(&usage.data, "2026-11-11", "2026-10-13", &mut p);
        assert_eq!(
            p.today,
            AnthropicFigures { input_tokens: 10, cache_read_tokens: 700, cache_write_tokens: 0, output_tokens: 5 }
        );
        assert_eq!(p.last_30_days, p.today);
        assert_eq!(p.all_time.input_tokens, 110);
        assert_eq!(p.all_time.cache_write_tokens, 50);

        let cost: AnPage<AnCost> = serde_json::from_str(
            r#"{"data":[{"starting_at":"2026-11-11T00:00:00Z","results":[{"amount":"123.45","currency":"USD"},
                                                                       {"amount":"0.55","currency":"USD"}]},
                        {"starting_at":"2026-10-01T00:00:00Z","results":[{"amount":"200","currency":"USD"}]}],
                "has_more":false}"#,
        )
        .unwrap();
        let mut usd = Periods::default();
        add_cost(&cost.data, "2026-11-11", "2026-10-13", &mut usd);
        assert!((usd.today - 1.24).abs() < 1e-9, "cents to dollars: {}", usd.today);
        assert!((usd.all_time - 3.24).abs() < 1e-9);
    }
}
