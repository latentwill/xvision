use anyhow::Result;

use crate::autooptimizer::eval_adapter::PaperTestRunner;
use crate::eval::scenario::Scenario;
use crate::strategies::Strategy;

#[derive(Debug, Clone)]
pub struct WindowOutcome {
    pub scenario_id: String,
    pub net_return_pct: f64,
    pub sharpe: f64,
    pub n_trades: usize,
}
#[derive(Debug, Clone)]
pub struct PoolEvaluation {
    pub outcomes: Vec<WindowOutcome>,
    pub profitable_fraction: f64,
    pub mean_net_return_pct: f64,
    /// Standard error of per-window child−parent deltas (noise floor).
    pub noise_floor: f64,
    /// Windows where child net return beats parent net return.
    pub wins_vs_parent: usize,
}
#[derive(Debug, Clone)]
pub struct PoolComparison {
    pub child: PoolEvaluation,
    pub parent: PoolEvaluation,
}

/// Evaluate one strategy on every day and holdout window in a scenario pool.
/// Each pair is evaluated with the same paper tester used by the cycle's normal
/// single-pair path; the returned outcomes are net of inference cost whenever
/// the runner provides pricing data.
pub async fn evaluate_pool(
    paper_tester: &dyn PaperTestRunner,
    strategy: &Strategy,
    scenario_pool: &[(Scenario, Scenario)],
) -> Result<PoolEvaluation> {
    let mut outcomes = Vec::with_capacity(scenario_pool.len() * 2);
    for (day, baseline) in scenario_pool {
        let day_metrics = paper_tester.run(strategy, day).await?;
        outcomes.push(window_outcome(day, &day_metrics));
        let baseline_metrics = paper_tester.run(strategy, baseline).await?;
        outcomes.push(window_outcome(baseline, &baseline_metrics));
    }
    Ok(summarize(outcomes, 0.0, 0))
}

/// Compare matching scenario windows from a child and parent evaluation.
/// Matching uses scenario id, preserving pool order and tolerating a missing
/// window in either side without inventing a zero-valued result.
pub fn compare_pool_evaluations(child: PoolEvaluation, parent: &PoolEvaluation) -> PoolComparison {
    let parent_by_id = parent
        .outcomes
        .iter()
        .map(|outcome| (outcome.scenario_id.as_str(), outcome.net_return_pct))
        .collect::<std::collections::HashMap<_, _>>();
    let deltas = child
        .outcomes
        .iter()
        .filter_map(|outcome| {
            parent_by_id
                .get(outcome.scenario_id.as_str())
                .map(|parent_return| outcome.net_return_pct - parent_return)
        })
        .collect::<Vec<_>>();
    let noise_floor = standard_error(&deltas);
    let wins_vs_parent = deltas.iter().filter(|delta| **delta > 0.0).count();
    let child = summarize(child.outcomes, noise_floor, wins_vs_parent);
    PoolComparison {
        child,
        parent: parent.clone(),
    }
}

fn window_outcome(
    scenario: &Scenario,
    metrics: &crate::eval::run::MetricsSummary,
) -> WindowOutcome {
    // Prefer the explicit formula so a runner cannot accidentally return a
    // gross value in a stale `net_return_pct` field.
    let net_return_pct = match (metrics.inference_cost_quote_total, scenario.capital.initial) {
        (Some(cost), capital) if capital > 0.0 => {
            metrics.total_return_pct - (cost / capital * 100.0)
        }
        _ => metrics.net_return_pct.unwrap_or(metrics.total_return_pct),
    };
    WindowOutcome {
        scenario_id: scenario.id.clone(),
        net_return_pct,
        sharpe: metrics.sharpe,
        n_trades: metrics.n_trades as usize,
    }
}

fn summarize(
    outcomes: Vec<WindowOutcome>,
    noise_floor: f64,
    wins_vs_parent: usize,
) -> PoolEvaluation {
    let profitable_fraction = if outcomes.is_empty() {
        0.0
    } else {
        outcomes
            .iter()
            .filter(|outcome| outcome.net_return_pct > 0.0)
            .count() as f64
            / outcomes.len() as f64
    };
    let mean_net_return_pct = if outcomes.is_empty() {
        0.0
    } else {
        outcomes
            .iter()
            .map(|outcome| outcome.net_return_pct)
            .sum::<f64>()
            / outcomes.len() as f64
    };
    PoolEvaluation {
        outcomes,
        profitable_fraction,
        mean_net_return_pct,
        noise_floor,
        wins_vs_parent,
    }
}

fn standard_error(deltas: &[f64]) -> f64 {
    if deltas.len() < 2 {
        return 0.0;
    }
    let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;
    let variance = deltas
        .iter()
        .map(|delta| {
            let centered = *delta - mean;
            centered * centered
        })
        .sum::<f64>()
        / (deltas.len() - 1) as f64;
    variance.sqrt() / (deltas.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{standard_error, summarize, WindowOutcome};

    #[test]
    fn noise_floor_is_sample_standard_error() {
        let deltas = [1.0, 3.0, 5.0];
        // sample std = 2; standard error = 2/sqrt(3).
        let expected = 2.0 / 3.0_f64.sqrt();
        assert!((standard_error(&deltas) - expected).abs() < 1e-12);
        assert_eq!(standard_error(&[1.0]), 0.0);
    }

    #[test]
    fn fraction_and_mean_use_all_windows() {
        let evaluation = summarize(
            vec![
                WindowOutcome {
                    scenario_id: "a".to_string(),
                    net_return_pct: 2.0,
                    sharpe: 0.0,
                    n_trades: 1,
                },
                WindowOutcome {
                    scenario_id: "b".to_string(),
                    net_return_pct: -1.0,
                    sharpe: 0.0,
                    n_trades: 1,
                },
                WindowOutcome {
                    scenario_id: "c".to_string(),
                    net_return_pct: 0.0,
                    sharpe: 0.0,
                    n_trades: 1,
                },
            ],
            0.0,
            0,
        );
        assert!((evaluation.profitable_fraction - 1.0 / 3.0).abs() < 1e-12);
        assert!((evaluation.mean_net_return_pct - 1.0 / 3.0).abs() < 1e-12);
    }
}
