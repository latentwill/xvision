//! Stage 5 — Filter v1 regression fixtures.
//!
//! Locks the deterministic-input side of the Filter v1 evaluation gate
//! described in `docs/superpowers/plans/2026-05-21-filter-v1.md` §"Stage
//! 5 — Regression fixtures".
//!
//! ## What this file gates today (Stage-1 surface only)
//!
//! * `scenario_btc_1h_300bars.json` is **byte-stable**: the committed
//!   JSON must match `generate_canonical_scenario()` exactly.
//! * The scenario has the expected shape (300 bars, strict 1h
//!   monotonic timestamps, no NaN / negative OHLC, high/low envelope
//!   the body).
//! * `filter_trend_pullback.toml` parses and validates against Stage
//!   1's `parse_toml` + `validate` surface — same DSL shape Stage 2's
//!   runtime will consume.
//!
//! ## Filter event gate
//!
//! `golden_filter_events_match_recorded_jsonl` runs the deterministic
//! scenario through the runtime and locks the byte-exact `FilterEventV1`
//! JSONL plus the aggregate `FilterSummary`.
//!
//! See `tests/fixtures/README.md` for the regeneration procedure.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use xvision_filters::{
    parse_toml, validate, Bar as RuntimeBar, EvalContext, FilterEventV1, FilterState, FilterSummary,
    RuntimeFilter,
};

const SCENARIO_JSON: &str = include_str!("fixtures/scenario_btc_1h_300bars.json");
const FILTER_TOML: &str = include_str!("fixtures/filter_trend_pullback.toml");
const EXPECTED_EVENTS_JSONL: &str = include_str!("fixtures/expected_events.jsonl");
const EXPECTED_SUMMARY_JSON: &str = include_str!("fixtures/expected_summary.json");

const SEED: u64 = 0x0F11_7E12_5EED_0001;
const WARMUP_BARS: usize = 200;
const DECISION_BARS: usize = 100;
const START_TS: &str = "2025-01-01T00:00:00Z";
const START_CLOSE: f64 = 50_000.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct FixtureScenario {
    symbol: String,
    timeframe: String,
    seed: u64,
    warmup_bars: usize,
    decision_bars: usize,
    bars: Vec<Bar>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Bar {
    ts: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[test]
fn scenario_fixture_matches_canonical_generator() {
    let loaded: FixtureScenario =
        serde_json::from_str(SCENARIO_JSON).expect("scenario fixture must deserialise");
    let canonical = generate_canonical_scenario();

    assert_eq!(
        loaded, canonical,
        "scenario_btc_1h_300bars.json drifted from generate_canonical_scenario(). \
         Either revert the change or run \
         `cargo test -p xvision-filters --test golden_determinism -- \
         --ignored regenerate_scenario` and commit the regenerated file."
    );
}

#[test]
fn scenario_has_expected_shape() {
    let loaded: FixtureScenario = serde_json::from_str(SCENARIO_JSON).unwrap();
    assert_eq!(loaded.symbol, "BTC/USD");
    assert_eq!(loaded.timeframe, "1h");
    assert_eq!(loaded.seed, SEED);
    assert_eq!(loaded.warmup_bars, WARMUP_BARS);
    assert_eq!(loaded.decision_bars, DECISION_BARS);
    assert_eq!(loaded.bars.len(), WARMUP_BARS + DECISION_BARS);
}

#[test]
fn scenario_has_strict_1h_monotonic_timestamps() {
    let loaded: FixtureScenario = serde_json::from_str(SCENARIO_JSON).unwrap();
    let timestamps: Vec<DateTime<Utc>> = loaded
        .bars
        .iter()
        .map(|b| {
            DateTime::parse_from_rfc3339(&b.ts)
                .unwrap_or_else(|e| panic!("bar ts '{}' must be RFC3339: {e}", b.ts))
                .with_timezone(&Utc)
        })
        .collect();

    assert_eq!(timestamps[0].to_rfc3339(), "2025-01-01T00:00:00+00:00");
    for w in timestamps.windows(2) {
        assert_eq!((w[1] - w[0]).num_seconds(), 3600, "bars must be exactly 1h apart");
    }
}

#[test]
fn scenario_has_well_formed_ohlc() {
    let loaded: FixtureScenario = serde_json::from_str(SCENARIO_JSON).unwrap();
    for (i, b) in loaded.bars.iter().enumerate() {
        for (name, v) in [
            ("open", b.open),
            ("high", b.high),
            ("low", b.low),
            ("close", b.close),
            ("volume", b.volume),
        ] {
            assert!(v.is_finite(), "bar {i} {name} must be finite, got {v}");
            assert!(v >= 0.0, "bar {i} {name} must be non-negative, got {v}");
        }
        let body_max = b.open.max(b.close);
        let body_min = b.open.min(b.close);
        assert!(
            b.high >= body_max,
            "bar {i}: high {} < max(open={}, close={})",
            b.high,
            b.open,
            b.close
        );
        assert!(
            b.low <= body_min,
            "bar {i}: low {} > min(open={}, close={})",
            b.low,
            b.open,
            b.close
        );
    }
}

#[test]
fn filter_trend_pullback_parses_and_validates() {
    let f = parse_toml(FILTER_TOML).expect("trend_pullback filter must parse");
    validate(&f).expect("trend_pullback filter must validate");
}

#[test]
fn golden_filter_events_match_recorded_jsonl() {
    let (events, summary) = build_golden_filter_events();
    let actual_jsonl = events_to_jsonl(&events);
    assert_eq!(actual_jsonl, EXPECTED_EVENTS_JSONL);

    let mut actual_summary = serde_json::to_string_pretty(&summary).expect("serialise golden filter summary");
    actual_summary.push('\n');
    assert_eq!(actual_summary, EXPECTED_SUMMARY_JSON);
}

/// Regenerate `tests/fixtures/scenario_btc_1h_300bars.json` from the
/// canonical generator. Ignored by default — run explicitly when a
/// deliberate generator change lands:
///
/// ```bash
/// cargo test -p xvision-filters --test golden_determinism -- \
///   --ignored regenerate_scenario
/// ```
#[test]
#[ignore = "writes tests/fixtures/scenario_btc_1h_300bars.json; run with --ignored"]
fn regenerate_scenario() {
    let canonical = generate_canonical_scenario();
    let mut json = serde_json::to_string_pretty(&canonical).expect("serialise canonical scenario");
    json.push('\n');
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/scenario_btc_1h_300bars.json"
    );
    std::fs::write(path, json).expect("write fixture");
}

/// Regenerate `tests/fixtures/expected_events.jsonl` and
/// `tests/fixtures/expected_summary.json` after an intentional runtime
/// or event-shape change:
///
/// ```bash
/// cargo test -p xvision-filters --test golden_determinism -- \
///   --ignored regenerate_filter_event_fixtures
/// ```
#[test]
#[ignore = "writes expected filter event fixtures; run with --ignored"]
fn regenerate_filter_event_fixtures() {
    let (events, summary) = build_golden_filter_events();
    let events_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expected_events.jsonl"
    );
    std::fs::write(events_path, events_to_jsonl(&events)).expect("write expected_events.jsonl");

    let mut summary_json = serde_json::to_string_pretty(&summary).expect("serialise summary");
    summary_json.push('\n');
    let summary_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expected_summary.json"
    );
    std::fs::write(summary_path, summary_json).expect("write expected_summary.json");
}

fn build_golden_filter_events() -> (Vec<FilterEventV1>, FilterSummary) {
    let filter = parse_toml(FILTER_TOML).expect("trend_pullback filter must parse");
    validate(&filter).expect("trend_pullback filter must validate");
    let scenario: FixtureScenario = serde_json::from_str(SCENARIO_JSON).unwrap();
    let runtime = RuntimeFilter::from_validated(&filter);
    let mut state = FilterState::new(&filter);
    let mut events = Vec::with_capacity(scenario.bars.len());

    for bar in &scenario.bars {
        let ts = DateTime::parse_from_rfc3339(&bar.ts)
            .expect("bar ts must be RFC3339")
            .with_timezone(&Utc);
        let runtime_bar = RuntimeBar::with_volume(bar.open, bar.high, bar.low, bar.close, bar.volume);
        let outcome = runtime.evaluate(
            &mut state,
            &runtime_bar,
            EvalContext {
                ts,
                in_position: false,
            },
        );
        events.push(FilterEventV1::from_outcome(
            filter.id.clone(),
            ts,
            &outcome,
            state.indicator_snapshot(&filter),
        ));
    }

    let summary = FilterSummary::from_events(filter.id.clone(), &events);
    (events, summary)
}

fn events_to_jsonl(events: &[FilterEventV1]) -> String {
    let mut jsonl = events
        .iter()
        .map(|event| serde_json::to_string(event).expect("serialise filter event"))
        .collect::<Vec<_>>()
        .join("\n");
    jsonl.push('\n');
    jsonl
}

// ---------------------------------------------------------------------------
// Canonical generator. Deterministic across macOS / Linux, ≥ rust 1.74.
// ---------------------------------------------------------------------------

fn generate_canonical_scenario() -> FixtureScenario {
    let start = DateTime::parse_from_rfc3339(START_TS)
        .expect("START_TS")
        .with_timezone(&Utc);

    let mut rng = Xorshift64::new(SEED);
    let mut bars = Vec::with_capacity(WARMUP_BARS + DECISION_BARS);
    let mut prev_close = START_CLOSE;

    for i in 0..(WARMUP_BARS + DECISION_BARS) {
        // Body: ±2% step, rounded to 2 decimals.
        let drift = (rng.next_unit() - 0.5) * 0.04;
        let open = prev_close;
        let close = round2(open * (1.0 + drift));

        // Wicks: bounded by body magnitude + a small constant, scaled by
        // a second draw so a flat body still gets a non-zero range.
        let body_max = open.max(close);
        let body_min = open.min(close);
        let body_range = body_max - body_min;
        let wick_budget = body_range + 30.0;
        let high = round2(body_max + rng.next_unit() * wick_budget);
        let low = round2((body_min - rng.next_unit() * wick_budget).max(0.0));

        let volume = round2(100.0 + rng.next_unit() * 1000.0);

        let ts = (start + Duration::hours(i as i64)).to_rfc3339_opts(SecondsFormat::Secs, true);

        bars.push(Bar {
            ts,
            open: round2(open),
            high,
            low,
            close,
            volume,
        });
        prev_close = close;
    }

    FixtureScenario {
        symbol: "BTC/USD".into(),
        timeframe: "1h".into(),
        seed: SEED,
        warmup_bars: WARMUP_BARS,
        decision_bars: DECISION_BARS,
        bars,
    }
}

#[inline]
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Tiny xorshift64. Deterministic and portable; suffices for fixture
/// generation (we do not need cryptographic strength).
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Xorshift64 with state 0 is degenerate; bias seed so the
        // caller can pass any u64 safely.
        Self {
            state: if seed == 0 { 0xDEAD_BEEF_CAFE_F00D } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform on [0, 1). Uses the top 53 bits — same trick the
    /// `rand` crate uses.
    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}
