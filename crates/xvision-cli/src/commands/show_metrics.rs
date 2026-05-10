//! `xvn show-metrics` — print headline numbers from a `BacktestResult` JSON.

use std::path::PathBuf;

use xvision_eval::result::BacktestResult;

pub fn run(report_path: PathBuf) -> anyhow::Result<()> {
    let bytes = std::fs::read(&report_path)?;
    let result: BacktestResult = serde_json::from_slice(&bytes)?;

    println!("XVISION backtest report — {}", report_path.display());
    println!("  cycles evaluated: {}", result.cycles_evaluated);
    println!("  initial NAV:      ${:.2}", result.initial_nav_usd);
    println!("  started:  {}", result.started_at);
    println!("  finished: {}", result.finished_at);
    println!();
    println!("Per-arm results:");
    for (name, arm) in &result.arms {
        let realized = arm.realized_pnl_total_usd;
        let n_dec = arm.decisions.len();
        let n_fills = arm.fills.len();
        let final_nav = arm
            .equity_curve
            .last()
            .map(|p| p.nav_usd)
            .unwrap_or(result.initial_nav_usd);
        println!(
            "  {:<24} decisions={:>4} fills={:>4} realized=${:>10.2} final_nav=${:>10.2}",
            name, n_dec, n_fills, realized, final_nav
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use chrono::Utc;
    use xvision_eval::result::{ArmResult, BacktestResult};

    #[test]
    fn show_metrics_reads_a_fixture_report() {
        let result = BacktestResult {
            arms: BTreeMap::from([(
                "trader_arm".to_string(),
                ArmResult {
                    name: "trader_arm".into(),
                    equity_curve: vec![],
                    fills: vec![],
                    decisions: vec![],
                    risk_outcomes: vec![],
                    returns: vec![],
                    realized_pnl_total_usd: 123.45,
                    regimes: vec![],
                },
            )]),
            cycles_evaluated: 0,
            initial_nav_usd: 100_000.0,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), serde_json::to_vec(&result).unwrap()).unwrap();
        run(tmp.path().to_path_buf()).expect("show_metrics should succeed");
    }
}
