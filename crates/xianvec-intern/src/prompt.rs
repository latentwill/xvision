//! Deterministic prompt builder. Same input → same string, byte-for-byte.
//! Snapshot-tested so a regression on the prompt itself is caught.

use std::fmt::Write;

use xianvec_core::{IndicatorPanel, MarketSnapshot, OnchainPanel, Regime, SkillRef};

#[derive(Debug, Clone)]
pub struct PromptOpts {
    /// Maximum number of recent OHLCV bars to include in the prompt.
    pub recent_bars_limit: usize,
}

impl Default for PromptOpts {
    fn default() -> Self {
        Self {
            recent_bars_limit: 12,
        }
    }
}

/// Build the Intern prompt. Output is deterministic given identical inputs.
///
/// The prompt is structured as:
/// 1. Role + non-recommendation rule
/// 2. Market context (asset, price, volume, regime, horizon)
/// 3. Recent OHLCV (last N bars)
/// 4. Indicators panel (skips None fields)
/// 5. Onchain panel (skips None fields)
/// 6. Loaded skills (catalog + name + summary)
/// 7. Required output schema (JSON only) with field descriptions
pub fn build_intern_prompt(state: &MarketSnapshot, skills: &[SkillRef], opts: &PromptOpts) -> String {
    let mut s = String::with_capacity(2048);

    s.push_str(SYSTEM_PREAMBLE);
    s.push_str("\n\n# Market context\n");
    let _ = writeln!(s, "- Asset: {}", state.asset.as_str());
    let _ = writeln!(s, "- Setup ID: {}", state.setup_id);
    let _ = writeln!(s, "- Timestamp (UTC): {}", state.timestamp.to_rfc3339());
    let _ = writeln!(s, "- Current price: {:.2}", state.price);
    if let Some(v) = state.volume_24h {
        let _ = writeln!(s, "- 24h volume (USD): {:.0}", v);
    }
    let _ = writeln!(s, "- Regime classifier: {}", regime_label(state.regime));
    let _ = writeln!(s, "- Horizon (hours): {}", state.horizon_hours);

    if !state.recent_bars.is_empty() {
        s.push_str("\n# Recent bars (chronological, oldest first)\n");
        let bars = state.recent_bars.iter().rev().take(opts.recent_bars_limit).rev();
        s.push_str("ts | open | high | low | close | volume\n");
        for b in bars {
            let _ = writeln!(
                s,
                "{} | {:.2} | {:.2} | {:.2} | {:.2} | {:.0}",
                b.timestamp.to_rfc3339(),
                b.open,
                b.high,
                b.low,
                b.close,
                b.volume
            );
        }
    }

    write_indicators(&mut s, &state.indicators);
    write_onchain(&mut s, &state.onchain);

    if !skills.is_empty() {
        s.push_str("\n# Loaded skill catalogs (domain context the desk has access to)\n");
        let mut sorted: Vec<&SkillRef> = skills.iter().collect();
        sorted.sort_by(|a, b| a.catalog.cmp(&b.catalog).then(a.name.cmp(&b.name)));
        for sk in sorted {
            let _ = writeln!(s, "- {}/{}: {}", sk.catalog, sk.name, sk.summary);
        }
    }

    s.push_str(SCHEMA_INSTRUCTIONS);
    s
}

fn write_indicators(s: &mut String, p: &IndicatorPanel) {
    let any = p.rsi_14.is_some()
        || p.sma_20.is_some()
        || p.sma_50.is_some()
        || p.sma_200.is_some()
        || p.ema_12.is_some()
        || p.ema_26.is_some()
        || p.bb_upper.is_some()
        || p.atr_14.is_some()
        || p.macd.is_some()
        || p.donchian_upper.is_some();
    if !any {
        return;
    }
    s.push_str("\n# Indicators\n");
    if let Some(v) = p.rsi_14 {
        let _ = writeln!(s, "- RSI(14): {:.2}", v);
    }
    if let Some(v) = p.sma_20 {
        let _ = writeln!(s, "- SMA(20): {:.2}", v);
    }
    if let Some(v) = p.sma_50 {
        let _ = writeln!(s, "- SMA(50): {:.2}", v);
    }
    if let Some(v) = p.sma_200 {
        let _ = writeln!(s, "- SMA(200): {:.2}", v);
    }
    if let Some(v) = p.ema_12 {
        let _ = writeln!(s, "- EMA(12): {:.2}", v);
    }
    if let Some(v) = p.ema_26 {
        let _ = writeln!(s, "- EMA(26): {:.2}", v);
    }
    if let (Some(u), Some(m), Some(l)) = (p.bb_upper, p.bb_middle, p.bb_lower) {
        let _ = writeln!(
            s,
            "- Bollinger(20,2σ): upper={:.2} mid={:.2} lower={:.2}",
            u, m, l
        );
    }
    if let Some(v) = p.atr_14 {
        let _ = writeln!(s, "- ATR(14): {:.2}", v);
    }
    if let (Some(m), Some(g), Some(h)) = (p.macd, p.macd_signal, p.macd_hist) {
        let _ = writeln!(s, "- MACD(12/26/9): macd={:.4} signal={:.4} hist={:.4}", m, g, h);
    }
    if let (Some(u), Some(l)) = (p.donchian_upper, p.donchian_lower) {
        let _ = writeln!(s, "- Donchian(20): upper={:.2} lower={:.2}", u, l);
    }
}

fn write_onchain(s: &mut String, p: &OnchainPanel) {
    let any = p.funding_rate_8h.is_some()
        || p.open_interest_usd.is_some()
        || p.long_short_ratio.is_some()
        || p.stablecoin_inflows_24h_usd.is_some()
        || p.liquidations_24h_usd.is_some()
        || p.realized_volatility_30d.is_some();
    if !any {
        return;
    }
    s.push_str("\n# Onchain & derivatives\n");
    if let Some(v) = p.funding_rate_8h {
        let _ = writeln!(s, "- Funding rate (8h): {:.6}", v);
    }
    if let Some(v) = p.open_interest_usd {
        let _ = writeln!(s, "- Open interest (USD): {:.0}", v);
    }
    if let Some(v) = p.long_short_ratio {
        let _ = writeln!(s, "- Long/short ratio: {:.3}", v);
    }
    if let Some(v) = p.stablecoin_inflows_24h_usd {
        let _ = writeln!(s, "- Stablecoin exchange inflows (24h, USD): {:.0}", v);
    }
    if let Some(v) = p.liquidations_24h_usd {
        let _ = writeln!(s, "- Liquidations (24h, USD): {:.0}", v);
    }
    if let Some(v) = p.realized_volatility_30d {
        let _ = writeln!(s, "- Realized vol (30d): {:.4}", v);
    }
}

fn regime_label(r: Regime) -> &'static str {
    match r {
        Regime::Bull => "bull",
        Regime::Bear => "bear",
        Regime::Chop => "chop",
        Regime::HighVol => "high_vol",
        Regime::LowVol => "low_vol",
    }
}

const SYSTEM_PREAMBLE: &str = "You are a senior market analyst writing a balanced briefing for the trading desk.\n\
Your single job is to surface the strongest case for each of {bull, bear, flat} given the data below, with supporting evidence tags.\n\
\n\
HARD RULES — violation will fail downstream parsing:\n\
1. You MUST NOT recommend a direction. Do NOT include a `candidate_direction` field.\n\
   Do NOT lean toward one case in any of the case strings. Each case must read as if its author believed it.\n\
2. Output JSON only. No prose, no commentary, no markdown fences.\n\
3. Schema is fixed. Extra fields will be rejected.\n\
4. `bull_case`, `bear_case`, `flat_case` MUST each be 20-2000 characters.";

const SCHEMA_INSTRUCTIONS: &str = "\n\n# Required output (JSON only)\n\
```\n\
{\n\
  \"bull_case\":  string  // 20-2000 chars; strongest bullish thesis given the data\n\
  \"bear_case\":  string  // 20-2000 chars; strongest bearish thesis given the data\n\
  \"flat_case\":  string  // 20-2000 chars; strongest no-trade thesis given the data\n\
  \"evidence_long\":  [ {\"kind\": \"technical|onchain|macro|sentiment|fundamental\", \"detail\": \"<short tag>\"} ]\n\
  \"evidence_short\": [ {\"kind\": \"technical|onchain|macro|sentiment|fundamental\", \"detail\": \"<short tag>\"} ]\n\
  \"evidence_flat\":  [ {\"kind\": \"technical|onchain|macro|sentiment|fundamental\", \"detail\": \"<short tag>\"} ]\n\
  \"signal_quality\": number  // 0.0-1.0; how confident you are that the data supports any meaningful read\n\
}\n\
```\n\
\n\
Emit only the JSON object. The `setup_id`, `asset`, `regime`, and `horizon_hours` fields are filled in by the runtime — do not include them.";

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;
    use xianvec_core::market::Ohlcv;
    use xianvec_core::trading::{AssetSymbol, Regime};

    fn fixture_state() -> MarketSnapshot {
        MarketSnapshot {
            setup_id: Uuid::nil(),
            asset: AssetSymbol::Btc,
            timestamp: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            price: 70_123.45,
            volume_24h: Some(28_000_000_000.0),
            recent_bars: (0..3)
                .map(|i| Ohlcv {
                    timestamp: chrono::Utc
                        .timestamp_opt(1_700_000_000 - (3 - i) * 3600, 0)
                        .single()
                        .unwrap(),
                    open: 70_000.0 + i as f64 * 100.0,
                    high: 70_500.0 + i as f64 * 100.0,
                    low: 69_900.0 + i as f64 * 100.0,
                    close: 70_200.0 + i as f64 * 100.0,
                    volume: 1_000_000.0,
                })
                .collect(),
            indicators: IndicatorPanel {
                rsi_14: Some(54.3),
                sma_20: Some(69_500.0),
                bb_upper: Some(72_000.0),
                bb_middle: Some(70_000.0),
                bb_lower: Some(68_000.0),
                ..Default::default()
            },
            onchain: OnchainPanel {
                funding_rate_8h: Some(0.000123),
                open_interest_usd: Some(9_200_000_000.0),
                ..Default::default()
            },
            regime: Regime::Chop,
            horizon_hours: 24,
        }
    }

    fn fixture_skills() -> Vec<SkillRef> {
        vec![
            SkillRef {
                catalog: "byreal".into(),
                name: "perp-risk-shapes".into(),
                summary: "Drawdown, funding skew, liquidation cascades.".into(),
            },
            SkillRef {
                catalog: "mantle".into(),
                name: "network-primer".into(),
                summary: "Mantle L2 throughput, fee structure, bridge UX.".into(),
            },
        ]
    }

    #[test]
    fn prompt_is_deterministic_byte_for_byte() {
        let s = fixture_state();
        let sk = fixture_skills();
        let opts = PromptOpts::default();
        let a = build_intern_prompt(&s, &sk, &opts);
        let b = build_intern_prompt(&s, &sk, &opts);
        assert_eq!(a, b);
        // Order-independent: skill list reordered must produce identical output.
        let mut sk2 = sk.clone();
        sk2.reverse();
        let c = build_intern_prompt(&s, &sk2, &opts);
        assert_eq!(a, c, "skill list ordering must not affect prompt");
    }

    #[test]
    fn prompt_forbids_direction_recommendation() {
        let p = build_intern_prompt(&fixture_state(), &[], &PromptOpts::default());
        assert!(p.contains("MUST NOT recommend a direction"));
        assert!(p.contains("candidate_direction"));
    }

    #[test]
    fn prompt_omits_unset_indicator_lines() {
        let mut s = fixture_state();
        s.indicators = IndicatorPanel::default();
        s.onchain = OnchainPanel::default();
        let p = build_intern_prompt(&s, &[], &PromptOpts::default());
        assert!(!p.contains("# Indicators"));
        assert!(!p.contains("# Onchain"));
    }

    #[test]
    fn prompt_includes_setup_id_and_asset() {
        let p = build_intern_prompt(&fixture_state(), &[], &PromptOpts::default());
        assert!(p.contains("Setup ID: 00000000-0000-0000-0000-000000000000"));
        assert!(p.contains("Asset: BTC"));
    }

    #[test]
    fn prompt_caps_recent_bars() {
        let mut s = fixture_state();
        s.recent_bars = (0..50)
            .map(|i| Ohlcv {
                timestamp: chrono::Utc
                    .timestamp_opt(1_700_000_000 - i * 3600, 0)
                    .single()
                    .unwrap(),
                open: 70_000.0,
                high: 70_500.0,
                low: 69_900.0,
                close: 70_200.0,
                volume: 1_000.0,
            })
            .collect();
        let p = build_intern_prompt(&s, &[], &PromptOpts { recent_bars_limit: 5 });
        let bar_lines = p.matches("70200.00").count();
        assert_eq!(bar_lines, 5, "must cap at recent_bars_limit");
    }

    #[test]
    fn prompt_strict_rules_present() {
        let p = build_intern_prompt(&fixture_state(), &[], &PromptOpts::default());
        for needle in [
            "Output JSON only",
            "20-2000",
            "evidence_long",
            "evidence_short",
            "evidence_flat",
            "signal_quality",
        ] {
            assert!(p.contains(needle), "missing: {needle}");
        }
    }
}
