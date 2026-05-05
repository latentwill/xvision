//! Tool surface advertised over MCP.
//!
//! Every tool is stateless: the caller supplies the price / HLC series as
//! parameters and we dispatch into `xianvec-data`. NaN positions in the
//! output mark indicator warmup and travel through the wire as JSON
//! `null` (serde's default for `f64::NAN` is `null` when wrapped in a
//! sentinel-aware type — we round-trip through `Option<f64>` for that).

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use xianvec_data as xvn;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn nan_to_null(xs: Vec<f64>) -> Vec<Option<f64>> {
    xs.into_iter()
        .map(|v| if v.is_finite() { Some(v) } else { None })
        .collect()
}

fn json_or_err<T: serde::Serialize>(t: &T) -> Result<String, rmcp::ErrorData> {
    serde_json::to_string(t).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))
}

// ---------------------------------------------------------------------------
// request shapes — one per tool. JsonSchema derives feed the tools/list
// schema agents see.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PricesPeriod {
    /// Closing prices, oldest → newest.
    pub prices: Vec<f64>,
    /// Lookback window (e.g. 14 for RSI, 20 for SMA-20).
    pub period: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BollingerReq {
    /// Closing prices, oldest → newest.
    pub prices: Vec<f64>,
    /// Window length, e.g. 20.
    pub period: usize,
    /// Standard-deviation multiplier, e.g. 2.0.
    pub k: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AtrReq {
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    /// Wilder-smoothing period (typically 14).
    pub period: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacdReq {
    pub prices: Vec<f64>,
    /// Fast EMA period — canonical 12.
    pub fast: usize,
    /// Slow EMA period — canonical 26.
    pub slow: usize,
    /// Signal line EMA period — canonical 9.
    pub signal: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DonchianReq {
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub period: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FibReq {
    pub prices: Vec<f64>,
    /// How many recent bars to scan for the swing high / swing low.
    pub lookback: usize,
}

// ---------------------------------------------------------------------------
// XianvecTools — the rmcp router.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct XianvecTools;

#[tool_router(server_handler)]
impl XianvecTools {
    pub fn new() -> Self {
        Self
    }

    /// Server health + version probe. Returns a JSON object with
    /// `{ ok: true, name, version }`. Use to confirm the MCP wiring is
    /// live before issuing real tool calls.
    #[tool(description = "Health probe for the xianvec MCP server. Returns server name and version.")]
    fn xvn_health(&self) -> Result<String, rmcp::ErrorData> {
        json_or_err(&serde_json::json!({
            "ok": true,
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    /// Simple moving average. Returns a same-length series; warmup
    /// positions are JSON null. The latest SMA value is the last
    /// non-null element.
    #[tool(description = "Simple moving average over a closing-price series. Returns a same-length array; warmup positions are null.")]
    fn xvn_sma(&self, Parameters(req): Parameters<PricesPeriod>) -> Result<String, rmcp::ErrorData> {
        json_or_err(&nan_to_null(xvn::sma(&req.prices, req.period)))
    }

    /// Exponential moving average. Same shape as SMA.
    #[tool(description = "Exponential moving average over a closing-price series. EMA seeded with the SMA of the first `period` bars.")]
    fn xvn_ema(&self, Parameters(req): Parameters<PricesPeriod>) -> Result<String, rmcp::ErrorData> {
        json_or_err(&nan_to_null(xvn::ema(&req.prices, req.period)))
    }

    /// Wilder RSI on closing prices.
    #[tool(description = "Wilder-smoothed RSI on a closing-price series. Period 14 is canonical.")]
    fn xvn_rsi(&self, Parameters(req): Parameters<PricesPeriod>) -> Result<String, rmcp::ErrorData> {
        json_or_err(&nan_to_null(xvn::rsi(&req.prices, req.period)))
    }

    /// Bollinger Bands. Returns `{ middle: [...], upper: [...], lower: [...] }`.
    #[tool(description = "Bollinger Bands. Returns middle/upper/lower same-length arrays; warmup positions are null.")]
    fn xvn_bollinger(&self, Parameters(req): Parameters<BollingerReq>) -> Result<String, rmcp::ErrorData> {
        let bb = xvn::bollinger(&req.prices, req.period, req.k);
        json_or_err(&serde_json::json!({
            "middle": nan_to_null(bb.middle),
            "upper":  nan_to_null(bb.upper),
            "lower":  nan_to_null(bb.lower),
        }))
    }

    /// Wilder ATR. Inputs must be equal-length OHLC series.
    #[tool(description = "Wilder-smoothed Average True Range. Inputs (high/low/close) must be equal-length series.")]
    fn xvn_atr(&self, Parameters(req): Parameters<AtrReq>) -> Result<String, rmcp::ErrorData> {
        if req.high.len() != req.low.len() || req.low.len() != req.close.len() {
            return Err(rmcp::ErrorData::invalid_params(
                "high/low/close must be equal length".to_string(),
                None,
            ));
        }
        json_or_err(&nan_to_null(xvn::atr(&req.high, &req.low, &req.close, req.period)))
    }

    /// MACD. Returns `{ macd: [...], signal: [...], histogram: [...] }`.
    #[tool(description = "MACD. Standard parameters fast=12, slow=26, signal=9. Returns macd/signal/histogram arrays.")]
    fn xvn_macd(&self, Parameters(req): Parameters<MacdReq>) -> Result<String, rmcp::ErrorData> {
        let m = xvn::macd(&req.prices, req.fast, req.slow, req.signal);
        json_or_err(&serde_json::json!({
            "macd":      nan_to_null(m.macd),
            "signal":    nan_to_null(m.signal),
            "histogram": nan_to_null(m.histogram),
        }))
    }

    /// Donchian channel — rolling-window high and low.
    #[tool(description = "Donchian channel. Rolling-period high and low over equal-length high/low arrays.")]
    fn xvn_donchian(&self, Parameters(req): Parameters<DonchianReq>) -> Result<String, rmcp::ErrorData> {
        if req.high.len() != req.low.len() {
            return Err(rmcp::ErrorData::invalid_params(
                "high/low must be equal length".to_string(),
                None,
            ));
        }
        let d = xvn::donchian(&req.high, &req.low, req.period);
        json_or_err(&serde_json::json!({
            "upper": nan_to_null(d.upper),
            "lower": nan_to_null(d.lower),
        }))
    }

    /// Fibonacci retracement levels. Detects the most recent swing in the
    /// lookback window and returns `(high, low, direction, levels)`.
    /// `levels` is an array of `[ratio, price]` pairs at the canonical
    /// ratios 0.236 / 0.382 / 0.500 / 0.618 / 0.786.
    #[tool(description = "Fibonacci retracement levels for the most recent swing within a lookback window.")]
    fn xvn_fib_retracements(&self, Parameters(req): Parameters<FibReq>) -> Result<String, rmcp::ErrorData> {
        match xvn::fib_retracements(&req.prices, req.lookback) {
            None => json_or_err(&serde_json::json!({ "found": false })),
            Some(f) => {
                let direction = match f.direction {
                    xvn::Direction::Up => "up",
                    xvn::Direction::Down => "down",
                };
                json_or_err(&serde_json::json!({
                    "found": true,
                    "high":      f.high,
                    "low":       f.low,
                    "direction": direction,
                    "levels": f.levels.iter().map(|(r, p)| serde_json::json!([r, p])).collect::<Vec<_>>(),
                }))
            }
        }
    }
}
