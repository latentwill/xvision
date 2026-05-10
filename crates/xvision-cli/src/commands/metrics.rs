//! `xvn metrics` and `xvn gate` — pre-committed metrics + anti-overfit verdict
//! computed from a `BacktestResult` JSON.

use std::path::PathBuf;

use xvision_eval::gate::anti_overfit_verdict;
use xvision_eval::metrics::compute_pre_committed;
use xvision_eval::result::BacktestResult;

pub fn run_metrics(
    report: PathBuf,
    treatment: String,
    baseline: String,
    n_resamples: usize,
    block_size: Option<usize>,
) -> anyhow::Result<()> {
    let result: BacktestResult = serde_json::from_slice(&std::fs::read(&report)?)?;
    let metrics = compute_pre_committed(&result, &treatment, &baseline, n_resamples, block_size)?;
    println!("{}", serde_json::to_string_pretty(&metrics)?);
    Ok(())
}

pub fn run_gate(
    report: PathBuf,
    treatment: String,
    baseline: String,
    n_resamples: usize,
    block_size: Option<usize>,
) -> anyhow::Result<()> {
    let result: BacktestResult = serde_json::from_slice(&std::fs::read(&report)?)?;
    let metrics = compute_pre_committed(&result, &treatment, &baseline, n_resamples, block_size)?;
    let verdict = anti_overfit_verdict(&metrics);
    println!("{}", serde_json::to_string_pretty(&verdict)?);
    Ok(())
}
