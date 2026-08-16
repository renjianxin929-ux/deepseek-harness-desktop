//! DeepSeek Harness Desktop — measured usage + estimated cost (read-only).
//!
//! This module computes **measured usage** from the user's `~/.dsh` data and a
//! **clearly-labelled estimated cost** from a versioned pricing snapshot. It is
//! strictly read-only over `~/.dsh`: it never modifies the session schema, never
//! rewrites session logs, never stores cost inside Harness session data, and
//! never deletes/compacts user session records. If Desktop-specific cache
//! metadata were ever needed, it would live in the Desktop app-data area — but
//! this module needs none.
//!
//! Semantics:
//! * MEASURED  = actual observed runtime/session usage (token buckets, model,
//!   API-call count) read from the harness's own persisted data.
//! * ESTIMATED = locally calculated monetary cost from a dated pricing
//!   snapshot. It is NEVER labelled an official bill or charged amount.
//!   Unknown model/price ⇒ usage still shown, cost unavailable (no other
//!   model's price is substituted).

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::TimeZone;
use serde::{Deserialize, Serialize};

/// Injected read-only current-session beacon (self-contained JS).
pub const BEACON: &str = include_str!("usage_beacon.js");

// ---------------------------------------------------------------------------
// Versioned pricing snapshot
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PricingEntry {
    pub provider: String,
    pub model: String,
    /// Date the snapshot was taken (YYYY-MM-DD).
    pub effective_date: String,
    /// Source description / URL.
    pub source: String,
    pub currency: String,
    /// Pricing unit, e.g. "1M tokens".
    pub unit: String,
    /// Price per unit for input cache HIT tokens.
    pub input_cache_hit: f64,
    /// Price per unit for input cache MISS (uncached input) tokens.
    pub input_cache_miss: f64,
    /// Price per unit for output tokens.
    pub output: f64,
    /// Always true: this is a locally calculated estimate, not official billing.
    pub estimation: bool,
}

/// The versioned pricing snapshot. `snapshot_version` allows the schema/data to
/// evolve without silently changing numbers. Source: DeepSeek official API
/// pricing page, fetched 2026-08-15 (flat rates; the page also announces
/// peak/off-peak pricing effective 2026-08-16 16:00 UTC, recorded in the note
/// but not applied to these flat rates).
pub const PRICING_SNAPSHOT_VERSION: u32 = 1;

pub fn pricing_snapshot() -> Vec<PricingEntry> {
    vec![
        PricingEntry {
            provider: "deepseek-official".into(),
            model: "deepseek-v4-pro".into(),
            effective_date: "2026-08-15".into(),
            source: "https://api-docs.deepseek.com/quick_start/pricing/ (snapshot; not official live billing)".into(),
            currency: "USD".into(),
            unit: "1M tokens".into(),
            input_cache_hit: 0.003625,
            input_cache_miss: 0.435,
            output: 0.87,
            estimation: true,
        },
        PricingEntry {
            provider: "deepseek-official".into(),
            model: "deepseek-v4-flash".into(),
            effective_date: "2026-08-15".into(),
            source: "https://api-docs.deepseek.com/quick_start/pricing/ (snapshot; not official live billing)".into(),
            currency: "USD".into(),
            unit: "1M tokens".into(),
            input_cache_hit: 0.0028,
            input_cache_miss: 0.14,
            output: 0.28,
            estimation: true,
        },
    ]
}

pub fn lookup_price(provider: &str, model: &str) -> Option<PricingEntry> {
    pricing_snapshot()
        .into_iter()
        .find(|p| p.provider == provider && p.model == model)
}

// ---------------------------------------------------------------------------
// Token totals + cost math (pure)
// ---------------------------------------------------------------------------

/// Disjoint token buckets, mirroring the harness's `tokenUsage` projection.
/// `input_tokens` is uncached input only; cached input is `cache_read_tokens`
/// and `cache_write_tokens`; reasoning is already a subdivision of output.
///
/// Serialized across the `get_usage` Tauri boundary in camelCase (the frontend
/// JSON contract); Rust-internal field identifiers remain snake_case.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenTotals {
    pub fn total(self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }

    pub fn cached_input(self) -> u64 {
        self.cache_read_tokens.saturating_add(self.cache_write_tokens)
    }
}

/// Estimated cost in the snapshot's currency (USD). Input buckets are disjoint:
/// uncached input is billed at the cache-miss rate, cache read+write at the
/// cache-hit rate, output at the output rate. Reasoning is not double-counted.
pub fn estimate_cost(totals: TokenTotals, price: &PricingEntry) -> f64 {
    let miss = totals.input_tokens as f64 / 1_000_000.0 * price.input_cache_miss;
    let hit = totals.cached_input() as f64 / 1_000_000.0 * price.input_cache_hit;
    let out = totals.output_tokens as f64 / 1_000_000.0 * price.output;
    miss + hit + out
}

// ---------------------------------------------------------------------------
// Harness persisted projection cache (read-only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ProjCacheFile {
    tables: ProjCacheTables,
}

#[derive(Debug, Deserialize)]
struct ProjCacheTables {
    sessions: BTreeMap<String, ProjCacheSession>,
}

#[derive(Debug, Deserialize)]
struct ProjCacheSession {
    identity: ProjCacheIdentity,
    rows: ProjCacheRows,
}

#[derive(Debug, Deserialize)]
struct ProjCacheIdentity {
    #[serde(rename = "createdAt")]
    created_at: u64,
}

#[derive(Debug, Deserialize)]
struct ProjCacheRows {
    #[serde(rename = "tokenUsage", default)]
    token_usage: Option<TokenUsageRow>,
}

#[derive(Debug, Deserialize)]
struct TokenUsageRow {
    #[serde(default)]
    val: Option<TokenUsageValue>,
}

#[derive(Debug, Deserialize)]
struct TokenUsageValue {
    #[serde(default)]
    totals: TokenUsageTotals,
}

#[derive(Debug, Deserialize, Default)]
struct TokenUsageTotals {
    #[serde(rename = "uncachedInputTokens", default)]
    uncached_input_tokens: u64,
    #[serde(rename = "outputTokens", default)]
    output_tokens: u64,
    #[serde(rename = "cacheReadTokens", default)]
    cache_read_tokens: u64,
    #[serde(rename = "cacheWriteTokens", default)]
    cache_write_tokens: u64,
}

/// Resolve the Harness home directory (`DSH_HOME`), defaulting to
/// `$HOME/.dsh`. Read-only: callers must never write through this path.
pub fn dsh_home(home: &Path) -> PathBuf {
    home.join(".dsh")
}

fn projcache_path(dsh: &Path) -> PathBuf {
    dsh.join("storages").join("session_projcache.json")
}

/// Session-id → token totals, read from the harness's persisted projection
/// cache. Missing/unreadable/malformed entries are simply absent (never a
/// panic, never a fabricated zero).
fn read_projcache_totals(dsh: &Path) -> BTreeMap<String, (u64, TokenTotals)> {
    let mut out = BTreeMap::new();
    let Ok(text) = fs::read_to_string(projcache_path(dsh)) else {
        return out;
    };
    let Ok(file) = serde_json::from_str::<ProjCacheFile>(&text) else {
        return out;
    };
    for (id, sess) in file.tables.sessions {
        let created_at = sess.identity.created_at;
        if let Some(tu) = sess.rows.token_usage.and_then(|r| r.val) {
            out.insert(
                id,
                (
                    created_at,
                    TokenTotals {
                        input_tokens: tu.totals.uncached_input_tokens,
                        output_tokens: tu.totals.output_tokens,
                        cache_read_tokens: tu.totals.cache_read_tokens,
                        cache_write_tokens: tu.totals.cache_write_tokens,
                    },
                ),
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Session log (zstd JSONL) — model identity + API-call count
// ---------------------------------------------------------------------------

/// Decode a `session.jsonl.zstd` artifact (concatenated zstd frames, one JSONL
/// record per frame, possibly batched) into its plaintext. A torn/incomplete
/// final frame (the actively-written session) is tolerated: complete frames are
/// returned, the remainder is ignored. Corrupt data yields best-effort partial
/// text rather than a panic.
fn decode_session_log(path: &Path) -> Option<String> {
    let f = fs::File::open(path).ok()?;
    let mut decoder = zstd::stream::read::Decoder::new(f).ok()?;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match decoder.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buf[..n]),
            Err(_) => break, // torn/incomplete tail — keep what decoded
        }
    }
    if bytes.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Model identity (`provider`, `model`) from the first `request/context` event,
/// and the number of completed model calls (`assistant/message` events).
struct LogSummary {
    provider: Option<String>,
    model: Option<String>,
    api_calls: u64,
}

fn summarize_log(text: &str) -> LogSummary {
    let mut summary = LogSummary {
        provider: None,
        model: None,
        api_calls: 0,
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        match ty {
            "request/context" => {
                if summary.provider.is_none() {
                    summary.provider = v
                        .get("data")
                        .and_then(|d| d.get("provider"))
                        .and_then(|x| x.as_str())
                        .map(String::from);
                    summary.model = v
                        .get("data")
                        .and_then(|d| d.get("model"))
                        .and_then(|x| x.as_str())
                        .map(String::from);
                }
            }
            "assistant/message" => summary.api_calls += 1,
            _ => {}
        }
    }
    summary
}

/// Locate a session's log file under `~/.dsh/sessions/<projectKey>/<sessionId>/`.
fn find_session_log(dsh: &Path, session_id: &str) -> Option<PathBuf> {
    let sessions_root = dsh.join("sessions");
    let Ok(projects) = fs::read_dir(&sessions_root) else {
        return None;
    };
    for project in projects.flatten() {
        let log = project.path().join(session_id).join("session.jsonl.zstd");
        if log.is_file() {
            return Some(log);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Report model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSession {
    pub session_id: String,
    pub tokens: TokenTotals,
    pub total_tokens: u64,
    pub api_calls: Option<u64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Estimated cost in USD; `None` = unavailable (unknown model/price).
    pub estimated_cost_usd: Option<f64>,
    pub pricing_used: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub available: bool,
    pub note: Option<String>,
    pub current_session_id: Option<String>,
    pub current_session: Option<UsageSession>,
    pub today: Option<UsageSession>,
    pub pricing_snapshot_version: u32,
    pub pricing_note: String,
    pub estimation: bool,
}

/// Whether an epoch-millis timestamp falls in today's **local** calendar day.
///
/// The local calendar date is resolved with the platform's own timezone rules
/// (DST-aware) via `chrono::Local`, so "today" flips at local midnight — not at
/// UTC midnight (the previous implementation bucketed by UTC epoch-day and made
/// "today" flip at 08:00 local for UTC+8 users).
fn is_today_ms(ts_ms: u64) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let ts_date = local_date_of(ts_ms as i64);
    let now_date = local_date_of(now_ms);
    match (ts_date, now_date) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Resolve an epoch-millis timestamp to a local calendar date using the
/// platform timezone (`chrono::Local`). Returns `None` for out-of-range or
/// unrepresentable instants. `earliest()` resolves DST-ambiguous instants to a
/// concrete wall time (the calendar date is unchanged by that ambiguity).
fn local_date_of(ts_ms: i64) -> Option<chrono::NaiveDate> {
    chrono::Local
        .timestamp_millis_opt(ts_ms)
        .earliest()
        .map(|dt| dt.date_naive())
}

/// Deterministic seam: whether two epoch-millis instants fall on the same local
/// calendar date under a **fixed** UTC offset (seconds east of UTC). This is the
/// pure, timezone-independent core of "today" and is what the deterministic
/// boundary tests exercise. Test-only: production `is_today_ms` uses the
/// platform timezone via [`local_date_of`] (DST-aware).
#[cfg(test)]
fn same_local_day(ts_ms: i64, now_ms: i64, offset_secs: i32) -> bool {
    let off = chrono::FixedOffset::east_opt(offset_secs).unwrap_or_else(|| {
        chrono::FixedOffset::east_opt(0).expect("UTC offset always constructible")
    });
    let ts_date = off
        .timestamp_millis_opt(ts_ms)
        .earliest()
        .map(|dt| dt.date_naive());
    let now_date = off
        .timestamp_millis_opt(now_ms)
        .earliest()
        .map(|dt| dt.date_naive());
    match (ts_date, now_date) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn build_session_usage(
    session_id: String,
    tokens: TokenTotals,
    created_at_ms: u64,
    dsh: &Path,
    price_override: Option<&PricingEntry>,
) -> UsageSession {
    let log_summary = find_session_log(dsh, &session_id)
        .and_then(|p| decode_session_log(&p))
        .map(|text| summarize_log(&text));

    let provider = log_summary
        .as_ref()
        .and_then(|s| s.provider.clone())
        .or_else(|| price_override.map(|p| p.provider.clone()));
    let model = log_summary
        .as_ref()
        .and_then(|s| s.model.clone())
        .or_else(|| price_override.map(|p| p.model.clone()));

    let (estimated_cost_usd, pricing_used) = match (&provider, &model) {
        (Some(p), Some(m)) => match lookup_price(p, m) {
            Some(price) => (Some(estimate_cost(tokens, &price)), true),
            None => (None, false),
        },
        _ => (None, false),
    };

    UsageSession {
        session_id,
        tokens,
        total_tokens: tokens.total(),
        api_calls: log_summary.as_ref().map(|s| s.api_calls),
        provider,
        model,
        estimated_cost_usd,
        pricing_used,
        created_at_ms,
    }
}

/// Compute the read-only usage report for `dsh`. `current_session_id` is the
/// reliably-established current session (from the injected beacon); when
/// `None`, the current-session card is omitted (never guessed from timestamps).
pub fn compute_usage(dsh: &Path, current_session_id: Option<&str>) -> UsageReport {
    let totals = read_projcache_totals(dsh);

    let mut current_session = None;
    if let Some(id) = current_session_id {
        if let Some((created_at, toks)) = totals.get(id) {
            current_session = Some(build_session_usage(
                id.to_string(),
                *toks,
                *created_at,
                dsh,
                None,
            ));
        }
    }

    // "Today": aggregate only sessions whose createdAt is today's local day.
    let mut today_tokens = TokenTotals::default();
    let mut today_api_calls: u64 = 0;
    let mut today_models: BTreeMap<String, TokenTotals> = BTreeMap::new();
    let mut today_count: u64 = 0;
    let mut today_known_model_count: u64 = 0;
    for (id, (created_at, toks)) in &totals {
        if !is_today_ms(*created_at) {
            continue;
        }
        today_count += 1;
        today_tokens.input_tokens = today_tokens.input_tokens.saturating_add(toks.input_tokens);
        today_tokens.output_tokens = today_tokens.output_tokens.saturating_add(toks.output_tokens);
        today_tokens.cache_read_tokens =
            today_tokens.cache_read_tokens.saturating_add(toks.cache_read_tokens);
        today_tokens.cache_write_tokens =
            today_tokens.cache_write_tokens.saturating_add(toks.cache_write_tokens);

        let log_summary = find_session_log(dsh, id)
            .and_then(|p| decode_session_log(&p))
            .map(|text| summarize_log(&text));
        if let Some(ls) = &log_summary {
            today_api_calls = today_api_calls.saturating_add(ls.api_calls);
            if let (Some(p), Some(m)) = (&ls.provider, &ls.model) {
                today_known_model_count += 1;
                let key = format!("{p}/{m}");
                let e = today_models.entry(key).or_default();
                e.input_tokens = e.input_tokens.saturating_add(toks.input_tokens);
                e.output_tokens = e.output_tokens.saturating_add(toks.output_tokens);
                e.cache_read_tokens = e.cache_read_tokens.saturating_add(toks.cache_read_tokens);
                e.cache_write_tokens = e.cache_write_tokens.saturating_add(toks.cache_write_tokens);
            }
        }
    }

    // Today's cost: computable ONLY when every contributing session has a known
    // model and they all share one priced model; otherwise cost is unavailable
    // (honest — no substitution of another model's price, no partial pricing).
    let mut today_cost: Option<f64> = None;
    let mut today_pricing_used = false;
    if today_count > 0
        && today_known_model_count == today_count
        && today_models.len() == 1
    {
        let key = today_models.keys().next().unwrap();
        if let Some((p, m)) = key.split_once('/') {
            if let Some(price) = lookup_price(p, m) {
                let toks = today_models[key];
                today_cost = Some(estimate_cost(toks, &price));
                today_pricing_used = true;
            }
        }
    }

    let today = if today_count > 0 {
        // A single model identity is shown only when every contributing session
        // shares one known model; otherwise the model is "unknown/multiple".
        let (provider, model) = if today_known_model_count == today_count && today_models.len() == 1
        {
            let key = today_models.keys().next().unwrap();
            key.split_once('/')
                .map(|(p, m)| (Some(p.to_string()), Some(m.to_string())))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        Some(UsageSession {
            session_id: "TODAY".into(),
            tokens: today_tokens,
            total_tokens: today_tokens.total(),
            api_calls: Some(today_api_calls),
            provider,
            model,
            estimated_cost_usd: today_cost,
            pricing_used: today_pricing_used,
            created_at_ms: 0,
        })
    } else {
        None
    };

    UsageReport {
        available: true,
        note: None,
        current_session_id: current_session_id.map(String::from),
        current_session,
        today,
        pricing_snapshot_version: PRICING_SNAPSHOT_VERSION,
        pricing_note: "Estimated cost from a dated pricing snapshot (not official live billing). Unknown model/price ⇒ cost unavailable.".into(),
        estimation: true,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn get_usage_json_contract_is_camel_case() {
        let report = UsageReport {
            available: true,
            note: None,
            current_session_id: Some("session-abc".into()),
            current_session: Some(UsageSession {
                session_id: "session-abc".into(),
                tokens: TokenTotals {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_tokens: 30,
                    cache_write_tokens: 40,
                },
                total_tokens: 100,
                api_calls: Some(7),
                provider: Some("deepseek-official".into()),
                model: Some("deepseek-v4-pro".into()),
                estimated_cost_usd: Some(0.1234),
                pricing_used: true,
                created_at_ms: 123_456_789,
            }),
            today: None,
            pricing_snapshot_version: 1,
            pricing_note: "estimated".into(),
            estimation: true,
        };
        let json = serde_json::to_value(&report).expect("serialize UsageReport");

        // Top-level camelCase keys.
        for key in ["currentSessionId", "currentSession", "pricingSnapshotVersion", "pricingNote"] {
            assert!(json.get(key).is_some(), "missing top-level key {key}");
        }
        // Snake_case top-level keys must not leak across the boundary.
        for key in ["current_session_id", "pricing_snapshot_version"] {
            assert!(json.get(key).is_none(), "snake_case key {key} must not be emitted");
        }

        let session = &json["currentSession"];
        for key in [
            "sessionId",
            "totalTokens",
            "apiCalls",
            "estimatedCostUsd",
            "pricingUsed",
            "createdAtMs",
        ] {
            assert!(session.get(key).is_some(), "missing UsageSession key {key}");
        }
        for key in [
            "session_id",
            "total_tokens",
            "api_calls",
            "estimated_cost_usd",
            "pricing_used",
            "created_at_ms",
        ] {
            assert!(session.get(key).is_none(), "snake_case UsageSession key {key} must not be emitted");
        }

        let tokens = &session["tokens"];
        for key in ["inputTokens", "outputTokens", "cacheReadTokens", "cacheWriteTokens"] {
            assert!(tokens.get(key).is_some(), "missing TokenTotals key {key}");
        }
        for key in ["input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens"] {
            assert!(tokens.get(key).is_none(), "snake_case TokenTotals key {key} must not be emitted");
        }

        // Value spot-checks (contract, not math).
        assert_eq!(json["currentSessionId"], "session-abc");
        assert_eq!(session["sessionId"], "session-abc");
        assert_eq!(session["totalTokens"], 100);
        assert_eq!(session["apiCalls"], 7);
        assert_eq!(tokens["inputTokens"], 10);
    }

    #[test]
    fn cost_math_is_disjoint_and_consistent() {
        let p = lookup_price("deepseek-official", "deepseek-v4-pro").unwrap();
        let t = TokenTotals {
            input_tokens: 1_000_000,     // 1M cache miss
            output_tokens: 1_000_000,    // 1M output
            cache_read_tokens: 1_000_000,// 1M cache hit
            cache_write_tokens: 0,
        };
        let c = estimate_cost(t, &p);
        assert!((c - (0.435 + 0.87 + 0.003625)).abs() < 1e-9);
        assert_eq!(t.total(), 3_000_000);
        assert_eq!(t.cached_input(), 1_000_000);
    }

    #[test]
    fn zero_usage_is_zero_cost_not_fabricated() {
        let p = lookup_price("deepseek-official", "deepseek-v4-pro").unwrap();
        assert_eq!(estimate_cost(TokenTotals::default(), &p), 0.0);
    }

    #[test]
    fn unknown_model_has_no_price() {
        assert!(lookup_price("deepseek-official", "deepseek-v9-unknown").is_none());
        assert!(lookup_price("other-provider", "deepseek-v4-pro").is_none());
    }

    #[test]
    fn snapshot_entries_are_all_flagged_as_estimates() {
        for p in pricing_snapshot() {
            assert!(p.estimation, "{} must be flagged as an estimate", p.model);
            assert_eq!(p.currency, "USD");
        }
    }

    #[test]
    fn summarize_extracts_model_and_api_calls() {
        let text = r#"{"type":"request/context","data":{"provider":"deepseek-official","model":"deepseek-v4-pro"}}
{"type":"assistant/message","data":{"turn":1,"step":1}}
{"type":"assistant/message","data":{"turn":1,"step":2}}
{"type":"text-chunks","data":{}}
"#;
        let s = summarize_log(text);
        assert_eq!(s.provider.as_deref(), Some("deepseek-official"));
        assert_eq!(s.model.as_deref(), Some("deepseek-v4-pro"));
        assert_eq!(s.api_calls, 2);
    }

    #[test]
    fn malformed_log_lines_are_skipped() {
        let text = "not json\n{\"type\":\"assistant/message\"}\n{\"type\":\"request/context\",\"data\":{}}\n";
        let s = summarize_log(text);
        assert_eq!(s.api_calls, 1);
        assert!(s.model.is_none());
    }

    #[test]
    fn decode_concatenated_frames_yields_all_records() {
        // Two independently-written zstd frames (the on-disk harness format).
        let dir = std::env::temp_dir().join(format!("hd-usage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl.zstd");
        {
            let f = fs::File::create(&path).unwrap();
            let mut e1 = zstd::stream::write::Encoder::new(f, 3).unwrap();
            e1.write_all(b"{\"type\":\"session\",\"id\":\"s1\"}\n").unwrap();
            e1.finish().unwrap();
        }
        {
            let f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            let mut e2 = zstd::stream::write::Encoder::new(f, 3).unwrap();
            e2.write_all(b"{\"type\":\"assistant/message\"}\n").unwrap();
            e2.finish().unwrap();
        }
        let text = decode_session_log(&path).expect("decode");
        assert!(text.contains("\"type\":\"session\""));
        assert!(text.contains("\"type\":\"assistant/message\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_log_is_none_not_panic() {
        assert!(decode_session_log(Path::new("/nonexistent/never.zstd")).is_none());
    }

    #[test]
    fn now_is_today() {
        assert!(is_today_ms(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64));
    }

    // Deterministic local-calendar-day tests via the fixed-offset seam. The
    // epoch-millis values below are chosen so the *civil* day boundary is what
    // is being tested, independent of the machine's timezone.

    /// 2026-08-15 12:00:00 UTC, in ms.
    const NOON_UTC_MS: i64 = 1_786_795_200_000;

    #[test]
    fn same_local_day_matches_same_civil_day_utc() {
        // Both instants are 2026-08-15 in UTC (offset 0).
        assert!(same_local_day(NOON_UTC_MS, NOON_UTC_MS + 3_600_000, 0));
        // 2026-08-15 12:00 vs 2026-08-16 12:00 (one civil day later) => false.
        assert!(!same_local_day(NOON_UTC_MS, NOON_UTC_MS + 86_400_000, 0));
    }

    #[test]
    fn previous_local_day_is_not_today() {
        // now = 2026-08-15 12:00 UTC; ts = 2026-08-14 23:59 UTC. Under UTC+8
        // both are the same UTC civil day difference of ~12h, but under UTC the
        // two are on different civil days only if the boundary is crossed.
        // Use UTC to assert a clean "previous day" case.
        let now = NOON_UTC_MS; // 2026-08-15 12:00 UTC
        let prev = NOON_UTC_MS - 86_400_000; // 2026-08-14 12:00 UTC
        assert!(!same_local_day(prev, now, 0));
    }

    #[test]
    fn local_midnight_boundary_235959_to_000000() {
        // now = 2026-08-15 00:00:00 UTC (civil day boundary).
        let midnight_ms: i64 = NOON_UTC_MS - 43_200_000; // 2026-08-15 00:00 UTC
        // 23:59:59.999 the previous day => not the same local day (UTC).
        assert!(!same_local_day(midnight_ms - 1, midnight_ms, 0));
        // exactly 00:00:00 => same local day.
        assert!(same_local_day(midnight_ms, midnight_ms + 1, 0));
    }

    #[test]
    fn utc_plus_8_day_flip_happens_at_local_midnight() {
        // For UTC+8, local midnight is 16:00 UTC the previous day. A session at
        // 15:59 UTC (=23:59 local) is the *previous* local day; 16:00 UTC
        // (=00:00 local) is the *current* local day.
        let off = 8 * 3600;
        // now = 2026-08-15 00:00 local = 2026-08-14 16:00 UTC.
        let now = NOON_UTC_MS - 72_000_000; // 2026-08-14 16:00 UTC
        let before = now - 60_000; // 15:59 UTC => 23:59 local (prev day)
        let after = now; // 16:00 UTC => 00:00 local (today)
        assert!(!same_local_day(before, now, off));
        assert!(same_local_day(after, now, off));
    }

    #[test]
    fn negative_offset_day_flip() {
        // UTC-5: local midnight is 05:00 UTC. 04:59 UTC => prev local day,
        // 05:00 UTC => current local day.
        let off = -5 * 3600;
        let now = NOON_UTC_MS - 25_200_000; // 2026-08-15 05:00 UTC => 00:00 local
        assert!(!same_local_day(now - 60_000, now, off));
        assert!(same_local_day(now, now, off));
    }

    #[test]
    fn local_date_of_resolves_platform_timezone_date() {
        // is_today_ms on "now" must always be true (regardless of timezone).
        assert!(is_today_ms(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64));
    }

    #[test]
    fn beacon_is_read_only() {
        // The beacon must never mutate sessions or reach Tauri IPC; it only
        // reads the persisted selection and fires an image beacon.
        const FORBIDDEN: &[&str] = &[
            "session.prompt",
            "session.cancel",
            "session.create",
            "session.fork",
            "session.rename",
            "__TAURI__",
            "window.__TAURI_INTERNALS__",
        ];
        for token in FORBIDDEN {
            assert!(!BEACON.contains(token), "beacon must not contain {token}");
        }
        assert!(BEACON.contains("hd-usage-session://set"));
        assert!(BEACON.contains("\"dsh.sessions.current\""));
    }

    // ------------------------------------------------------------------
    // compute_usage fixtures
    // ------------------------------------------------------------------

    fn tmp_dsh(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hd-dsh-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zstd_log(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let f = fs::File::create(path).unwrap();
        let mut e = zstd::stream::write::Encoder::new(f, 3).unwrap();
        e.write_all(text.as_bytes()).unwrap();
        e.finish().unwrap();
    }

    fn write_projcache(dsh: &Path, sessions: &[(&str, u64, TokenTotals)]) {
        let storages = dsh.join("storages");
        fs::create_dir_all(&storages).unwrap();
        let mut map = serde_json::Map::new();
        for (id, created_at, toks) in sessions {
            let session = serde_json::json!({
                "identity": { "createdAt": created_at },
                "rows": {
                    "tokenUsage": {
                        "ver": 1, "seq": 1,
                        "val": {
                            "totals": {
                                "uncachedInputTokens": toks.input_tokens,
                                "outputTokens": toks.output_tokens,
                                "cacheReadTokens": toks.cache_read_tokens,
                                "cacheWriteTokens": toks.cache_write_tokens,
                            }
                        }
                    }
                }
            });
            map.insert(id.to_string(), session);
        }
        let file = serde_json::json!({ "tables": { "sessions": map } });
        fs::write(
            storages.join("session_projcache.json"),
            serde_json::to_string(&file).unwrap(),
        )
        .unwrap();
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    #[test]
    fn compute_usage_missing_dsh_is_available_and_empty() {
        let dsh = tmp_dsh("empty");
        let report = compute_usage(&dsh, None);
        assert!(report.available);
        assert!(report.current_session.is_none());
        assert!(report.today.is_none());
        let _ = fs::remove_dir_all(&dsh);
    }

    #[test]
    fn compute_usage_malformed_projcache_does_not_panic() {
        let dsh = tmp_dsh("malformed");
        fs::create_dir_all(dsh.join("storages")).unwrap();
        fs::write(dsh.join("storages").join("session_projcache.json"), "{ not json").unwrap();
        let report = compute_usage(&dsh, None);
        assert!(report.available);
        assert!(report.today.is_none());
        let _ = fs::remove_dir_all(&dsh);
    }

    #[test]
    fn compute_usage_resolves_current_session_with_cost() {
        let dsh = tmp_dsh("current");
        let id = "session-abc";
        let toks = TokenTotals {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
            cache_write_tokens: 0,
        };
        write_projcache(&dsh, &[(id, now_ms(), toks)]);
        write_zstd_log(
            &dsh.join("sessions").join("proj").join(id).join("session.jsonl.zstd"),
            "{\"type\":\"request/context\",\"data\":{\"provider\":\"deepseek-official\",\"model\":\"deepseek-v4-pro\"}}\n{\"type\":\"assistant/message\"}\n{\"type\":\"assistant/message\"}\n",
        );
        let report = compute_usage(&dsh, Some(id));
        let cur = report.current_session.as_ref().expect("current session");
        assert_eq!(cur.session_id, id);
        assert_eq!(cur.tokens, toks);
        assert_eq!(cur.model.as_deref(), Some("deepseek-v4-pro"));
        assert_eq!(cur.api_calls, Some(2));
        assert!(cur.estimated_cost_usd.is_some());
        assert!(cur.pricing_used);
        // Today must also be present (createdAt is now).
        assert!(report.today.is_some());
        let _ = fs::remove_dir_all(&dsh);
    }

    #[test]
    fn compute_usage_unknown_model_shows_usage_but_no_cost() {
        let dsh = tmp_dsh("unknownmodel");
        let id = "session-unk";
        let toks = TokenTotals {
            input_tokens: 100,
            ..Default::default()
        };
        write_projcache(&dsh, &[(id, now_ms(), toks)]);
        write_zstd_log(
            &dsh.join("sessions").join("proj").join(id).join("session.jsonl.zstd"),
            "{\"type\":\"request/context\",\"data\":{\"provider\":\"acme\",\"model\":\"mystery\"}}\n",
        );
        let report = compute_usage(&dsh, Some(id));
        let cur = report.current_session.as_ref().expect("current session");
        assert_eq!(cur.tokens, toks);
        assert_eq!(cur.model.as_deref(), Some("mystery"));
        assert!(cur.estimated_cost_usd.is_none());
        assert!(!cur.pricing_used);
        let _ = fs::remove_dir_all(&dsh);
    }

    #[test]
    fn compute_usage_partial_token_fields_default_to_zero() {
        let dsh = tmp_dsh("partial");
        let storages = dsh.join("storages");
        fs::create_dir_all(&storages).unwrap();
        // Only outputTokens present; others absent => zero, not panic.
        let file = serde_json::json!({
            "tables": { "sessions": {
                "session-partial": {
                    "identity": { "createdAt": now_ms() },
                    "rows": { "tokenUsage": { "val": { "totals": { "outputTokens": 42 } } } }
                }
            }}
        });
        fs::write(
            storages.join("session_projcache.json"),
            serde_json::to_string(&file).unwrap(),
        )
        .unwrap();
        let report = compute_usage(&dsh, None);
        let today = report.today.as_ref().expect("today");
        assert_eq!(today.tokens.output_tokens, 42);
        assert_eq!(today.tokens.input_tokens, 0);
        assert_eq!(today.tokens.cache_read_tokens, 0);
        assert_eq!(today.tokens.cache_write_tokens, 0);
        let _ = fs::remove_dir_all(&dsh);
    }

    #[test]
    fn compute_usage_multiple_sessions_aggregate_today() {
        let dsh = tmp_dsh("multi");
        let a = TokenTotals { input_tokens: 10, output_tokens: 5, ..Default::default() };
        let b = TokenTotals { input_tokens: 20, output_tokens: 5, ..Default::default() };
        write_projcache(&dsh, &[("s1", now_ms(), a), ("s2", now_ms(), b)]);
        // Two DIFFERENT models => today cost unavailable (no substitution).
        write_zstd_log(
            &dsh.join("sessions").join("p1").join("s1").join("session.jsonl.zstd"),
            "{\"type\":\"request/context\",\"data\":{\"provider\":\"deepseek-official\",\"model\":\"deepseek-v4-pro\"}}\n",
        );
        write_zstd_log(
            &dsh.join("sessions").join("p2").join("s2").join("session.jsonl.zstd"),
            "{\"type\":\"request/context\",\"data\":{\"provider\":\"deepseek-official\",\"model\":\"deepseek-v4-flash\"}}\n",
        );
        let report = compute_usage(&dsh, None);
        let today = report.today.as_ref().expect("today");
        assert_eq!(today.tokens.input_tokens, 30);
        assert_eq!(today.tokens.output_tokens, 10);
        assert!(today.estimated_cost_usd.is_none(), "mixed models must not fabricate a cost");
        let _ = fs::remove_dir_all(&dsh);
    }

    #[test]
    fn compute_usage_partial_model_coverage_does_not_price_unknown() {
        let dsh = tmp_dsh("partialmodel");
        let a = TokenTotals { input_tokens: 10, output_tokens: 5, ..Default::default() };
        let b = TokenTotals { input_tokens: 20, output_tokens: 5, ..Default::default() };
        write_projcache(&dsh, &[("s1", now_ms(), a), ("s2", now_ms(), b)]);
        // s1 has a known model; s2 has NO log (model unknown). Today's cost must
        // be unavailable — the known model's price must not be applied to s2's
        // tokens (no substitution).
        write_zstd_log(
            &dsh.join("sessions").join("p1").join("s1").join("session.jsonl.zstd"),
            "{\"type\":\"request/context\",\"data\":{\"provider\":\"deepseek-official\",\"model\":\"deepseek-v4-pro\"}}\n",
        );
        let report = compute_usage(&dsh, None);
        let today = report.today.as_ref().expect("today");
        assert_eq!(today.tokens.input_tokens, 30);
        assert!(today.estimated_cost_usd.is_none(), "must not price tokens of unknown model");
        assert!(today.model.is_none(), "mixed/unknown model identity must not be asserted");
        let _ = fs::remove_dir_all(&dsh);
    }
}
