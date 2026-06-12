use crate::eval::run::BaselinesReport;

pub const MIN_TRADES_FOR_EVIDENCE: u32 = 10;

#[derive(Debug, Clone, Copy)]
pub struct EvidenceInputs<'a> {
    pub n_trades: u32,
    pub return_ci_low: Option<f64>,
    pub baselines: Option<&'a BaselinesReport>,
    pub fees_modeled: bool,
}

pub fn evidence_grade(input: EvidenceInputs<'_>) -> &'static str {
    if input.n_trades == 0 || !input.fees_modeled {
        return "D";
    }

    let enough_trades = input.n_trades >= MIN_TRADES_FOR_EVIDENCE;
    let ci_excludes_zero = input.return_ci_low.is_some_and(|low| low > 0.0);
    let beats_buy_hold = input
        .baselines
        .is_some_and(|b| b.relative_to.buy_hold > 0.0);

    match (enough_trades, ci_excludes_zero, beats_buy_hold) {
        (true, true, true) => "A",
        (true, _, true) => "B",
        (true, true, _) => "B",
        (false, _, _) => "C",
        _ => "C",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run::{BaselineMetrics, BaselineRelative, BaselinesReport};

    fn baselines(relative_buy_hold: f64) -> BaselinesReport {
        let zero = BaselineMetrics {
            return_pct: 0.0,
            sharpe: 0.0,
        };
        BaselinesReport {
            buy_hold: zero.clone(),
            always_flat: zero.clone(),
            simple_trend: zero.clone(),
            simple_mean_reversion: zero.clone(),
            random_direction: zero,
            relative_to: BaselineRelative {
                buy_hold: relative_buy_hold,
                always_flat: 0.0,
                simple_trend: 0.0,
                simple_mean_reversion: 0.0,
                random_direction: 0.0,
            },
        }
    }

    #[test]
    fn grade_a_requires_trade_floor_positive_ci_and_buy_hold_beat() {
        let baselines = baselines(2.0);
        assert_eq!(
            evidence_grade(EvidenceInputs {
                n_trades: 12,
                return_ci_low: Some(0.1),
                baselines: Some(&baselines),
                fees_modeled: true,
            }),
            "A"
        );
    }

    #[test]
    fn sparse_nonzero_run_is_c() {
        assert_eq!(
            evidence_grade(EvidenceInputs {
                n_trades: 3,
                return_ci_low: Some(0.1),
                baselines: None,
                fees_modeled: true,
            }),
            "C"
        );
    }

    #[test]
    fn zero_trade_or_unmodeled_fees_is_d() {
        assert_eq!(
            evidence_grade(EvidenceInputs {
                n_trades: 0,
                return_ci_low: None,
                baselines: None,
                fees_modeled: true,
            }),
            "D"
        );
        assert_eq!(
            evidence_grade(EvidenceInputs {
                n_trades: 20,
                return_ci_low: Some(0.1),
                baselines: None,
                fees_modeled: false,
            }),
            "D"
        );
    }
}
