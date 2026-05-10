//! Deterministic Trader prompt builder. Same input → same string, byte-for-
//! byte. Snapshot-tested so a regression on the prompt itself is caught.
//!
//! Layout:
//! 1. Role + JSON-only directive
//! 2. The Intern's bull/bear/flat case + evidence + signal_quality + regime
//! 3. Portfolio state (equity, exposure, open positions)
//! 4. Required output schema with field constraints

use std::fmt::Write;

use xianvec_core::trading::{EvidenceTag, InternBriefing, OpenPosition, PortfolioState, Regime};

use crate::params::TraderParams;

#[derive(Debug, Clone)]
pub struct TraderPromptOpts {
    /// Cap on how many open positions to enumerate. v1 portfolios are tiny so
    /// the cap exists only to bound the prompt against future multi-asset use.
    pub max_open_positions: usize,
}

impl Default for TraderPromptOpts {
    fn default() -> Self {
        Self {
            max_open_positions: 16,
        }
    }
}

pub fn build_trader_prompt(
    briefing: &InternBriefing,
    portfolio: &PortfolioState,
    _params: &TraderParams,
    opts: &TraderPromptOpts,
) -> String {
    let mut s = String::with_capacity(2048);

    s.push_str(SYSTEM_PREAMBLE);

    s.push_str("\n\n# Intern briefing\n");
    let _ = writeln!(s, "- Asset: {}", briefing.asset.as_str());
    let _ = writeln!(s, "- Cycle ID: {}", briefing.cycle_id);
    let _ = writeln!(s, "- Regime: {}", regime_label(briefing.regime));
    let _ = writeln!(s, "- Horizon (hours): {}", briefing.horizon_hours);
    let _ = writeln!(s, "- Signal quality: {:.3}", briefing.signal_quality);

    s.push_str("\n## Bull case\n");
    s.push_str(&briefing.bull_case);
    s.push_str("\n\n## Bear case\n");
    s.push_str(&briefing.bear_case);
    s.push_str("\n\n## Flat case\n");
    s.push_str(&briefing.flat_case);

    write_evidence_block(&mut s, "Evidence supporting long", &briefing.evidence_long);
    write_evidence_block(&mut s, "Evidence supporting short", &briefing.evidence_short);
    write_evidence_block(&mut s, "Evidence supporting flat", &briefing.evidence_flat);

    s.push_str("\n# Portfolio state\n");
    let _ = writeln!(s, "- Equity (USD): {:.2}", portfolio.equity_usd);
    let _ = writeln!(
        s,
        "- Realized PnL today (USD): {:.2}",
        portfolio.realized_pnl_today_usd
    );
    let _ = writeln!(s, "- Day index: {}", portfolio.day_index);
    let _ = writeln!(
        s,
        "- Total open exposure (bps NAV): {}",
        portfolio.total_exposure_bps()
    );
    if portfolio.is_flat() {
        s.push_str("- Open positions: none (flat)\n");
    } else {
        s.push_str("- Open positions:\n");
        for op in portfolio.open_positions.values().take(opts.max_open_positions) {
            write_open_position(&mut s, op);
        }
    }

    s.push_str(SCHEMA_INSTRUCTIONS);

    s
}

fn write_evidence_block(s: &mut String, header: &str, ev: &[EvidenceTag]) {
    if ev.is_empty() {
        return;
    }
    let _ = writeln!(s, "\n## {header}");
    for tag in ev {
        let (kind, detail) = match tag {
            EvidenceTag::Technical(d) => ("technical", d),
            EvidenceTag::Onchain(d) => ("onchain", d),
            EvidenceTag::Macro(d) => ("macro", d),
            EvidenceTag::Sentiment(d) => ("sentiment", d),
            EvidenceTag::Fundamental(d) => ("fundamental", d),
        };
        let _ = writeln!(s, "- [{kind}] {detail}");
    }
}

fn write_open_position(s: &mut String, op: &OpenPosition) {
    let dir = match op.direction {
        xianvec_core::Direction::Long => "long",
        xianvec_core::Direction::Short => "short",
        xianvec_core::Direction::Flat => "flat",
    };
    let _ = writeln!(
        s,
        "  - {} {} {}bps @ entry={:.2} mark={:.2} stop_pct={:.2} tp_pct={:.2}",
        op.asset.as_str(),
        dir,
        op.size_bps,
        op.entry_price,
        op.mark_price,
        op.stop_loss_pct,
        op.take_profit_pct,
    );
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

const SYSTEM_PREAMBLE: &str = "You are the Trader. You read a balanced bull/bear/flat briefing from the Intern and emit a single concrete trade decision.\n\
\n\
HARD RULES — violation will fail downstream parsing:\n\
1. Output JSON only. No prose, no commentary, no markdown fences.\n\
2. Schema is fixed. Extra fields will be rejected.\n\
3. `action` ∈ {buy, sell, flat, close}. `direction` ∈ {long, short, flat}.\n\
4. `size_bps` ∈ [0, 2000] (basis points of NAV; max 20%).\n\
5. `stop_loss_pct` ∈ [0.1, 20.0]. `take_profit_pct` ∈ [0.1, 50.0]. Both required.\n\
6. `trader_summary` ∈ [10, 500] characters. One sentence, no JSON inside it.\n\
7. If you choose `flat`, set size_bps = 0, direction = flat, but still emit valid stop/tp values (use 0.1 / 0.1).";

const SCHEMA_INSTRUCTIONS: &str = "\n\n# Required output (JSON only)\n\
```\n\
{\n\
  \"action\":          \"buy\" | \"sell\" | \"flat\" | \"close\",\n\
  \"direction\":       \"long\" | \"short\" | \"flat\",\n\
  \"size_bps\":        integer in [0, 2000],\n\
  \"stop_loss_pct\":   number  in [0.1, 20.0],\n\
  \"take_profit_pct\": number  in [0.1, 50.0],\n\
  \"trader_summary\":  string  in [10, 500] chars\n\
}\n\
```\n\
\n\
Emit only the JSON object. The runtime will fill in `cycle_id`.";

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use xianvec_core::trading::{
        AssetSymbol, Direction, EvidenceTag, InternBriefing, OpenPosition, PortfolioState, Regime,
    };

    fn fixture_briefing() -> InternBriefing {
        InternBriefing {
            cycle_id: Uuid::nil(),
            asset: AssetSymbol::Btc,
            bull_case: "Funding rate compressed; smart money accumulating spot.".into(),
            bear_case: "Realized vol expanding; long-leverage approaching prior squeeze level.".into(),
            flat_case: "Range-bound between SMA20 and SMA50; await directional break.".into(),
            evidence_long: vec![EvidenceTag::Onchain("smart_money_inflow".into())],
            evidence_short: vec![EvidenceTag::Technical("rsi_overbought".into())],
            evidence_flat: vec![EvidenceTag::Technical("range_bound".into())],
            regime: Regime::Chop,
            signal_quality: 0.62,
            horizon_hours: 24,
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    fn fixture_portfolio_with_position() -> PortfolioState {
        let op = OpenPosition {
            asset: AssetSymbol::Btc,
            direction: Direction::Long,
            size_bps: 800,
            entry_price: 70_000.0,
            mark_price: 70_500.0,
            stop_loss_pct: 2.0,
            take_profit_pct: 5.0,
            opened_at: chrono::Utc.timestamp_opt(1_699_900_000, 0).single().unwrap(),
        };
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: -250.0,
            day_index: 7,
            open_positions: BTreeMap::from([(AssetSymbol::Btc, op)]),
            as_of: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    fn fixture_portfolio_flat() -> PortfolioState {
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: 0.0,
            day_index: 0,
            open_positions: BTreeMap::new(),
            as_of: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    #[test]
    fn prompt_is_deterministic_byte_for_byte() {
        let b = fixture_briefing();
        let p = fixture_portfolio_with_position();
        let params = TraderParams::default();
        let opts = TraderPromptOpts::default();
        let a = build_trader_prompt(&b, &p, &params, &opts);
        let b2 = build_trader_prompt(&b, &p, &params, &opts);
        assert_eq!(a, b2);
    }

    #[test]
    fn prompt_includes_required_schema_fields() {
        let s = build_trader_prompt(
            &fixture_briefing(),
            &fixture_portfolio_with_position(),
            &TraderParams::default(),
            &TraderPromptOpts::default(),
        );
        for needle in [
            "Output JSON only",
            "\"action\":",
            "\"direction\":",
            "\"size_bps\":",
            "\"stop_loss_pct\":",
            "\"take_profit_pct\":",
            "\"trader_summary\":",
            "[0, 2000]",
        ] {
            assert!(s.contains(needle), "missing: {needle}\n---\n{s}");
        }
    }

    #[test]
    fn prompt_renders_flat_portfolio() {
        let s = build_trader_prompt(
            &fixture_briefing(),
            &fixture_portfolio_flat(),
            &TraderParams::default(),
            &TraderPromptOpts::default(),
        );
        assert!(s.contains("Open positions: none (flat)"));
        assert!(!s.contains("entry="));
    }

    #[test]
    fn prompt_renders_open_position() {
        let s = build_trader_prompt(
            &fixture_briefing(),
            &fixture_portfolio_with_position(),
            &TraderParams::default(),
            &TraderPromptOpts::default(),
        );
        assert!(s.contains("BTC long 800bps"));
        assert!(s.contains("entry=70000.00"));
    }

    #[test]
    fn prompt_includes_briefing_signal_quality_and_regime() {
        let s = build_trader_prompt(
            &fixture_briefing(),
            &fixture_portfolio_flat(),
            &TraderParams::default(),
            &TraderPromptOpts::default(),
        );
        assert!(s.contains("Signal quality: 0.620"));
        assert!(s.contains("Regime: chop"));
    }

    #[test]
    fn prompt_omits_empty_evidence_blocks() {
        let mut b = fixture_briefing();
        b.evidence_long.clear();
        b.evidence_short.clear();
        b.evidence_flat.clear();
        let s = build_trader_prompt(
            &b,
            &fixture_portfolio_flat(),
            &TraderParams::default(),
            &TraderPromptOpts::default(),
        );
        assert!(!s.contains("Evidence supporting"));
    }
}
