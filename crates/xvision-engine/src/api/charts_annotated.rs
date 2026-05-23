//! AI-annotation chart payload (chart-rework spec Track B B3).
//!
//! Two endpoints:
//!   - `GET /api/v2/charts/annotated/:run_id` — annotations stored
//!     alongside a backtest run. B3 ships a fixture-backed stub that
//!     re-serves the frontend `annotations.json` + a generated
//!     candle column array.
//!   - `GET /api/v2/charts/annotated/live/:symbol` — on-demand
//!     annotation generation. The producer is **out of scope** for
//!     this wave (spec §9); the endpoint returns the candle shape
//!     with `annotations: []` and `source: "live"` so the UI can
//!     render an EmptyState explaining the producer is unwired.
//!
//! The candle column array is synthesised at runtime from the same
//! seeded PRNG the frontend fixture generator uses, so frontend tests
//! that mount the surface against either source see consistent shape
//! + length (170 hourly bars starting 2025-02-01).

use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandleColumns {
    pub time: Vec<f64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineSeries {
    pub time: Vec<f64>,
    pub value: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotation {
    pub idx: u32,
    pub side: String, // "top" | "bottom" — kept as string for forward-compat
    #[serde(rename = "type")]
    pub kind: String, // "PATTERN" | "FLOW" | "RISK" | "REVERSION" | "STRUCTURE"
    pub title: String,
    pub body: String,
    pub conf: f64,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danger: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnnotatedChartPayload {
    /// Always `"annotated"`.
    pub kind: String,
    /// Provenance per spec §11.2: `"run"` or `"live"`.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub asset: String,
    pub granularity: String,
    pub candles: CandleColumns,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ema: Option<LineSeries>,
    /// May be `[]` when `source == "live"` and the producer is not
    /// wired (the typical case until the annotation producer ships).
    pub annotations: Vec<Annotation>,
}

// ── Deterministic candle generator ───────────────────────────────────────

/// mulberry32 — same PRNG the frontend fixture script uses.
fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut a: u32 = seed;
    move || {
        a = a.wrapping_add(0x6d2b79f5);
        let mut t = a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

/// Generate 170 hourly candles starting at unix-seconds for 2025-02-01.
/// Anchors the fixture annotations (idx 22, 52, 80, 110, 144) within
/// the candle range so the frontend rendering looks correct.
pub fn build_demo_candles() -> CandleColumns {
    const COUNT: usize = 170;
    const STEP_SEC: f64 = 3600.0;
    // 2025-02-01T00:00:00Z = 1738368000
    const START_SEC: f64 = 1_738_368_000.0;
    const START_PRICE: f64 = 63_500.0;
    const VOL: f64 = 280.0; // wick magnitude
    let mut rng = mulberry32(17);

    let mut out = CandleColumns {
        time: Vec::with_capacity(COUNT),
        open: Vec::with_capacity(COUNT),
        high: Vec::with_capacity(COUNT),
        low: Vec::with_capacity(COUNT),
        close: Vec::with_capacity(COUNT),
        volume: Vec::with_capacity(COUNT),
    };
    let mut price = START_PRICE;
    for i in 0..COUNT {
        let drift =
            (((i as f64) / 14.0).sin() * 90.0) + (((i as f64) / 35.0).cos() * 240.0);
        let prev_drift = if i > 0 {
            let p = (i - 1) as f64;
            (p / 14.0).sin() * 90.0 + (p / 35.0).cos() * 240.0
        } else {
            0.0
        };
        let open = price;
        let noise = (rng() - 0.5) * 2.0 * 210.0;
        let close = open + noise + (drift - prev_drift);
        let high = open.max(close) + rng() * VOL;
        let low = open.min(close) - rng() * VOL;
        let vol = 800.0 + rng() * 1800.0;
        out.time.push(START_SEC + (i as f64) * STEP_SEC);
        out.open.push(round2(open));
        out.high.push(round2(high));
        out.low.push(round2(low));
        out.close.push(round2(close));
        out.volume.push(round2(vol));
        price = close;
    }
    out
}

fn demo_annotations() -> Vec<Annotation> {
    vec![
        Annotation {
            idx: 22,
            side: "top".into(),
            kind: "PATTERN".into(),
            title: "Bull Flag".into(),
            body: "Flag consolidation after impulse. Breakout > 64,920 likely retests 63,100 wick.".into(),
            conf: 0.74,
            action: "WATCH".into(),
            danger: None,
            ts: Some(1_738_368_000.0 + 22.0 * 3600.0),
        },
        Annotation {
            idx: 52,
            side: "bottom".into(),
            kind: "FLOW".into(),
            title: "Volume Divergence".into(),
            body: "LL price with HH buy volume — accumulation footprint, 3-bar window.".into(),
            conf: 0.68,
            action: "LONG".into(),
            danger: None,
            ts: Some(1_738_368_000.0 + 52.0 * 3600.0),
        },
        Annotation {
            idx: 80,
            side: "top".into(),
            kind: "RISK".into(),
            title: "Liquidation Wall".into(),
            body: "$48M long liq cluster at 65,800. Likely magnet on next vol expansion.".into(),
            conf: 0.82,
            action: "CAUTION".into(),
            danger: Some(true),
            ts: Some(1_738_368_000.0 + 80.0 * 3600.0),
        },
        Annotation {
            idx: 110,
            side: "bottom".into(),
            kind: "REVERSION".into(),
            title: "RSI Reset".into(),
            body: "RSI cooled 71 → 47 without breaking trend. Mean-reversion re-entry zone.".into(),
            conf: 0.61,
            action: "LONG".into(),
            danger: None,
            ts: Some(1_738_368_000.0 + 110.0 * 3600.0),
        },
        Annotation {
            idx: 144,
            side: "top".into(),
            kind: "STRUCTURE".into(),
            title: "Break of Structure".into(),
            body: "HL → HH → BoS sequence confirmed. Bias flips bullish on close > 65,200.".into(),
            conf: 0.79,
            action: "LONG".into(),
            danger: None,
            ts: Some(1_738_368_000.0 + 144.0 * 3600.0),
        },
    ]
}

/// B3 stub: returns a `source: "run"` payload with demo candles and
/// the handoff's 5 sample annotations. Real builder reads annotations
/// stored alongside the run (follow-up; out of scope for this PR).
pub fn build_annotated_run_stub(run_id: &str) -> ApiResult<AnnotatedChartPayload> {
    if run_id.is_empty() {
        return Err(ApiError::Validation(
            "run_id must be a non-empty string".into(),
        ));
    }
    Ok(AnnotatedChartPayload {
        kind: "annotated".into(),
        source: "run".into(),
        run_id: Some(run_id.to_string()),
        symbol: None,
        asset: "BTC/USDT".into(),
        granularity: "1h".into(),
        candles: build_demo_candles(),
        ema: None,
        annotations: demo_annotations(),
    })
}

/// B3 stub: returns a `source: "live"` payload with demo candles and
/// **no** annotations. The live annotation producer is out of scope
/// (spec §9); the UI handles empty annotations with an EmptyState.
pub fn build_annotated_live_stub(symbol: &str) -> ApiResult<AnnotatedChartPayload> {
    if symbol.is_empty() {
        return Err(ApiError::Validation(
            "symbol must be a non-empty string".into(),
        ));
    }
    Ok(AnnotatedChartPayload {
        kind: "annotated".into(),
        source: "live".into(),
        run_id: None,
        symbol: Some(symbol.to_string()),
        asset: symbol.to_string(),
        granularity: "1h".into(),
        candles: build_demo_candles(),
        ema: None,
        annotations: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_candles_have_consistent_length() {
        let c = build_demo_candles();
        assert_eq!(c.time.len(), 170);
        assert_eq!(c.open.len(), 170);
        assert_eq!(c.high.len(), 170);
        assert_eq!(c.low.len(), 170);
        assert_eq!(c.close.len(), 170);
        assert_eq!(c.volume.len(), 170);
    }

    #[test]
    fn run_stub_parses_and_carries_5_annotations() {
        let p = build_annotated_run_stub("run-123").unwrap();
        assert_eq!(p.kind, "annotated");
        assert_eq!(p.source, "run");
        assert_eq!(p.run_id.as_deref(), Some("run-123"));
        assert!(p.symbol.is_none());
        assert_eq!(p.annotations.len(), 5);
        assert!(p.annotations.iter().any(|a| a.danger == Some(true)));
    }

    #[test]
    fn live_stub_returns_empty_annotations() {
        let p = build_annotated_live_stub("BTC/USDT").unwrap();
        assert_eq!(p.source, "live");
        assert_eq!(p.symbol.as_deref(), Some("BTC/USDT"));
        assert!(p.run_id.is_none());
        assert!(p.annotations.is_empty());
    }

    #[test]
    fn rejects_empty_inputs() {
        assert!(build_annotated_run_stub("").is_err());
        assert!(build_annotated_live_stub("").is_err());
    }

    #[test]
    fn payload_roundtrips_via_json() {
        let p = build_annotated_run_stub("r").unwrap();
        let s = serde_json::to_string(&p).unwrap();
        let back: AnnotatedChartPayload = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }
}
