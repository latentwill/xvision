//! WS1 live/backtest parity harness manifest.
//!
//! The runtime-entrypoint parity regressions live in
//! `eval_executor_live_loop.rs` because that file already owns the hermetic
//! `Executor::live` fixture stack: `MultiLiveStream`, `RunStore`,
//! `RealBrokerFills`, and broker mocks. This manifest keeps the named WS1
//! harness target in place and fails if the load-bearing parity cases are
//! removed from that executable suite.

const LIVE_LOOP_SUITE: &str = include_str!("eval_executor_live_loop.rs");

#[test]
fn ws1_live_parity_cases_are_encoded_in_live_loop_suite() {
    for case in [
        "live_loop_honors_deterministic_filter_gate",
        "live_loop_honors_strategy_decision_cadence",
        "live_loop_enforces_sltp_before_dispatch_and_counts_realized_round_trips",
        "live_loop_checks_sltp_on_non_cadence_bars",
        "live_loop_applies_broker_min_notional_before_submit",
        "live_loop_applies_short_borrow_cost_on_realized_close",
    ] {
        assert!(
            LIVE_LOOP_SUITE.contains(case),
            "WS1 live parity regression case `{case}` must remain encoded"
        );
    }
}
