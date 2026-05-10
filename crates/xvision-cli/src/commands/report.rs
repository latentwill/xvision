//! `xvn report` — render Markdown report from a `BacktestResult` JSON.

use std::path::PathBuf;

use xvision_eval::report::{render, ReportConfig};
use xvision_eval::result::BacktestResult;

pub fn run(input: PathBuf, output: PathBuf) -> anyhow::Result<()> {
    let bytes = std::fs::read(&input)?;
    let result: BacktestResult = serde_json::from_slice(&bytes)?;
    let cfg = ReportConfig::default();
    let md = render(&result, &cfg)?;
    std::fs::write(&output, md.as_bytes())?;
    println!("wrote report → {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use chrono::Utc;
    use xvision_core::trading::Regime;
    use xvision_eval::result::{ArmResult, BacktestResult};

    #[test]
    fn report_writes_markdown_to_disk() {
        let result = BacktestResult {
            arms: BTreeMap::from([
                (
                    "buy_and_hold".into(),
                    ArmResult {
                        name: "buy_and_hold".into(),
                        equity_curve: vec![],
                        fills: vec![],
                        decisions: vec![],
                        risk_outcomes: vec![],
                        returns: vec![0.001, -0.002, 0.003, -0.001, 0.002],
                        realized_pnl_total_usd: 100.0,
                        regimes: vec![
                            Regime::Bull,
                            Regime::Bull,
                            Regime::Bear,
                            Regime::Chop,
                            Regime::Bull,
                        ],
                    },
                ),
                (
                    "trader_arm".into(),
                    ArmResult {
                        name: "trader_arm".into(),
                        equity_curve: vec![],
                        fills: vec![],
                        decisions: vec![],
                        risk_outcomes: vec![],
                        returns: vec![0.002, -0.001, 0.004, 0.0, 0.003],
                        realized_pnl_total_usd: 500.0,
                        regimes: vec![
                            Regime::Bull,
                            Regime::Bull,
                            Regime::Bear,
                            Regime::Chop,
                            Regime::Bull,
                        ],
                    },
                ),
            ]),
            cycles_evaluated: 5,
            initial_nav_usd: 100_000.0,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        };
        let in_tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(in_tmp.path(), serde_json::to_vec(&result).unwrap()).unwrap();
        let out_tmp = tempfile::NamedTempFile::new().unwrap();
        run(in_tmp.path().to_path_buf(), out_tmp.path().to_path_buf())
            .expect("report must succeed on a 2-arm fixture");
        let md = std::fs::read_to_string(out_tmp.path()).unwrap();
        assert!(md.contains("Headline Δ-Sharpe"));
        assert!(md.contains("trader_arm"));
    }
}
