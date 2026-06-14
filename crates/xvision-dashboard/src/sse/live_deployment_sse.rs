//! SSE response builder for `GET /api/live/deployments/:id/stream` (CT5,
//! Epic s78 Wave 3, §4). Mirrors [`crate::sse::agent_run_sse`] (snapshot-first)
//! and the eval-runs `RunChartEvent` event bus: a deployment IS a live
//! `eval_runs` row, so this reuses the SAME `state.event_bus`
//! (`broadcast::Receiver<RunChartEvent>`) — no new broadcast surface.
//!
//! Flow (§4):
//! 1. Snapshot-first frame: `event: snapshot`, the full `LiveDeploymentSummary`.
//! 2. Per-event loop: map each `RunChartEvent` variant to a snake_case
//!    `event:` name; emit a synthetic `lagged` event on `RecvError::Lagged(n)`;
//!    break on the terminal `Status` lifecycle / channel close; 15s keep-alive.
//!
//! What the SSE carries vs. what the poll carries (CT5 §4, honest scope):
//! the stream delivers the full per-tick CT5 capital block on `event: metrics`
//! via `RunChartEvent::DeploymentMetrics` (`deployed_capital_usd`,
//! `unrealized_pnl_usd`, `realized_pnl_usd`, `daily_loss_limit_remaining_usd`,
//! `drawdown_pct`, `equity_usd`, `n_trades`) — null fields are OMITTED, never a
//! faked 0 — plus lifecycle/terminal `status` frames (bead s78.1). A bare
//! `RunChartEvent::Equity` tick still maps to `event: metrics` as an equity-only
//! heartbeat, so a client connecting before the first capital tick gets live
//! equity and degrades to the 5s poll (`GET /api/live/deployments`) for capital.
//! Still deferred: the `risk_veto` event (needs obs-event + last-visit tracking)
//! and 250ms batching.
//!
//! Terminal pre-check: when the run is ALREADY stopped at subscribe time, the
//! route handler builds the final snapshot and calls this builder in
//! terminal-only mode; it emits exactly ONE synthetic `status` frame carrying
//! that snapshot and ends, so a late subscriber never hangs on a freshly
//! re-created bus channel that will never fire (mirrors `eval_runs::stream`).

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::Stream;

use xvision_engine::api::chart::RunChartEvent;
use xvision_engine::api::live_deployments::LiveDeploymentSummary;

/// Map a `RunChartEvent` variant to its deployment-stream SSE `event:` name.
/// The frontend's `LIVE_SSE_EVENTS` const must match these exactly.
///
/// CT5 §4: the SSE `metrics` frame carries the full per-tick capital block via
/// `RunChartEvent::DeploymentMetrics` (`DeploymentMetricsTick` —
/// `{ time, equity_usd, drawdown_pct?, deployed_capital_usd?, unrealized_pnl_usd?,
/// realized_pnl_usd?, daily_loss_limit_remaining_usd?, n_trades }`). The bare
/// `RunChartEvent::Equity` tick ALSO maps to `metrics` (equity-only) so a client
/// that connects before the first capital tick still gets a live equity heartbeat
/// and DEGRADES to the poll for the capital fields. Both honest; null capital
/// fields are OMITTED, never a fabricated `0`.
pub fn event_name(ev: &RunChartEvent) -> &'static str {
    match ev {
        RunChartEvent::Bar(_) => "bar",
        RunChartEvent::IndicatorTail(_) => "indicator_tail",
        RunChartEvent::Decision(_) => "decision",
        RunChartEvent::Marker(_) => "marker",
        RunChartEvent::Equity(_) => "metrics",
        RunChartEvent::DeploymentMetrics(_) => "metrics",
        RunChartEvent::LiveRunState(_) => "live_run_state",
        RunChartEvent::Status { .. } => "status",
    }
}

/// True for the stream-closing lifecycle event. A live deployment's stream ends
/// when the run emits its terminal `Status` (the executor drops the channel
/// after this), matching `eval_runs::stream`.
fn is_terminal(ev: &RunChartEvent) -> bool {
    matches!(ev, RunChartEvent::Status { .. })
}

/// Serialize a `RunChartEvent` into the `data:` JSON for its SSE frame.
///
/// CT5 §4: the `DeploymentMetrics` capital tick serializes as its INNER
/// `DeploymentMetricsTick` (the flat `{ equity_usd, drawdown_pct?,
/// deployed_capital_usd?, unrealized_pnl_usd?, realized_pnl_usd?,
/// daily_loss_limit_remaining_usd?, n_trades }` contract the FE builds on), NOT
/// the `{event,data}` tagged envelope — the `event:` line already names the
/// frame. HONESTY MANDATE: null capital fields are OMITTED (the tick's
/// `skip_serializing_if`), never a fabricated `0`. All other variants keep the
/// existing tagged-envelope serialization so the equity / decision / status wire
/// contract is unchanged.
fn sse_payload(ev: &RunChartEvent) -> Result<String, serde_json::Error> {
    match ev {
        RunChartEvent::DeploymentMetrics(tick) => serde_json::to_string(tick),
        other => serde_json::to_string(other),
    }
}

/// Build the per-deployment SSE response. `snapshot` is the full
/// `LiveDeploymentSummary` at subscribe time; `rx` is the eval event bus
/// receiver for this run id (subscribed BEFORE the snapshot was assembled so no
/// event committed during assembly is lost).
///
/// When `terminal` is `true` the run was ALREADY stopped at subscribe time: the
/// executor has dropped the bus channel, so the recv loop would block forever on
/// a channel that will never fire. In that case this emits exactly ONE synthetic
/// `status` frame carrying the final snapshot and ends — late subscribers get
/// the terminal state and the stream closes (mirrors `eval_runs::stream`).
pub fn live_deployment_sse(
    snapshot: LiveDeploymentSummary,
    mut rx: broadcast::Receiver<RunChartEvent>,
    terminal: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let body = stream! {
        // Terminal pre-check: a run that was already stopped at subscribe time
        // never delivers bus events (the channel was dropped). Emit one
        // synthetic `status` frame with the final snapshot and end.
        if terminal {
            let payload = serde_json::to_string(&snapshot)
                .unwrap_or_else(|_| "{}".to_string());
            yield Ok(Event::default().event("status").data(payload));
            return;
        }

        // Snapshot first so the consumer always has full context before the
        // live tail starts.
        match serde_json::to_string(&snapshot) {
            Ok(payload) => {
                yield Ok(Event::default().event("snapshot").data(payload));
            }
            Err(e) => {
                let payload = json!({
                    "error": "snapshot_serialize_failed",
                    "message": e.to_string(),
                });
                let body = serde_json::to_string(&payload)
                    .unwrap_or_else(|_| "{\"error\":\"snapshot_serialize_failed\"}".into());
                yield Ok(Event::default().event("error").data(body));
            }
        }

        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let terminate = is_terminal(&ev);
                    let name = event_name(&ev);
                    match sse_payload(&ev) {
                        Ok(payload) => {
                            yield Ok(Event::default().event(name).data(payload));
                        }
                        // Serialization of a well-formed RunChartEvent should be
                        // infallible; skip on the unexpected failure rather than
                        // killing the stream.
                        Err(_) => continue,
                    }
                    if terminate {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let payload = json!({ "dropped": n });
                    let body = serde_json::to_string(&payload)
                        .unwrap_or_else(|_| "{\"dropped\":0}".into());
                    yield Ok(Event::default().event("lagged").data(body));
                    // Keep going — the client re-syncs via the REST snapshot.
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(body).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::api::chart::{
        ChartEquityPoint, DeploymentMetricsTick, LiveRunStatePayload, RunChartEvent,
    };

    fn full_tick() -> DeploymentMetricsTick {
        DeploymentMetricsTick {
            time: 1_700_000_000,
            equity_usd: 10_500.0,
            drawdown_pct: Some(2.5),
            deployed_capital_usd: Some(3_000.0),
            unrealized_pnl_usd: Some(120.0),
            realized_pnl_usd: Some(380.0),
            daily_loss_limit_remaining_usd: Some(450.0),
            n_trades: 4,
        }
    }

    #[test]
    fn event_names_match_run_chart_variants() {
        assert_eq!(
            event_name(&RunChartEvent::Equity(ChartEquityPoint {
                time: 0,
                equity_usd: 0.0
            })),
            "metrics"
        );
        // CT5 §4: the capital tick maps to the SAME `metrics` frame name.
        assert_eq!(
            event_name(&RunChartEvent::DeploymentMetrics(full_tick())),
            "metrics"
        );
        assert_eq!(
            event_name(&RunChartEvent::Status {
                phase: "running".into(),
                message: None
            }),
            "status"
        );
        assert_eq!(
            event_name(&RunChartEvent::LiveRunState(LiveRunStatePayload {
                equity_usd: None,
                unrealized_pnl_usd: None,
                realized_today_usd: None,
                daily_loss_remaining_usd: None,
                drawdown_pct: None,
                risk_veto_count: 0,
                last_decision_at: None,
            })),
            "live_run_state"
        );
    }

    #[test]
    fn metrics_frame_serializes_the_capital_block_not_just_equity() {
        // CT5 §4: the `metrics` frame's `data:` is the FLAT capital tick
        // (`{ equity_usd, drawdown_pct, deployed_capital_usd, ... }`), NOT the
        // `{event,data}` tagged envelope and NOT equity-only.
        let payload = sse_payload(&RunChartEvent::DeploymentMetrics(full_tick())).unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let obj = v.as_object().unwrap();
        // No tagged envelope — the inner tick is at the top level.
        assert!(!obj.contains_key("event"), "no tagged envelope, got {payload}");
        assert!(!obj.contains_key("data"), "no tagged envelope, got {payload}");
        assert_eq!(obj["equity_usd"], serde_json::json!(10_500.0));
        assert_eq!(obj["deployed_capital_usd"], serde_json::json!(3_000.0));
        assert_eq!(obj["unrealized_pnl_usd"], serde_json::json!(120.0));
        assert_eq!(obj["realized_pnl_usd"], serde_json::json!(380.0));
        assert_eq!(obj["daily_loss_limit_remaining_usd"], serde_json::json!(450.0));
        assert_eq!(obj["drawdown_pct"], serde_json::json!(2.5));
        assert_eq!(obj["n_trades"], serde_json::json!(4));
    }

    #[test]
    fn metrics_frame_omits_null_capital_fields_no_faked_zero() {
        // HONESTY MANDATE (§8.1): a null capital field is OMITTED, never `0`.
        let tick = DeploymentMetricsTick {
            time: 1_700_000_000,
            equity_usd: 10_000.0,
            drawdown_pct: None,
            deployed_capital_usd: None,
            unrealized_pnl_usd: None,
            realized_pnl_usd: None,
            daily_loss_limit_remaining_usd: None,
            n_trades: 0,
        };
        let payload = sse_payload(&RunChartEvent::DeploymentMetrics(tick)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let obj = v.as_object().unwrap();
        assert!(obj.contains_key("equity_usd"));
        assert!(
            !obj.contains_key("deployed_capital_usd"),
            "null omitted, got {payload}"
        );
        assert!(
            !obj.contains_key("realized_pnl_usd"),
            "null omitted, got {payload}"
        );
        assert!(
            !obj.contains_key("unrealized_pnl_usd"),
            "null omitted, got {payload}"
        );
        assert!(
            !obj.contains_key("daily_loss_limit_remaining_usd"),
            "null omitted, got {payload}"
        );
        assert!(!obj.contains_key("drawdown_pct"), "null omitted, got {payload}");
    }

    #[test]
    fn equity_tick_still_keeps_tagged_envelope() {
        // The bare equity heartbeat is UNCHANGED — tagged envelope preserved so
        // the client degrades to the poll for capital while still ticking equity.
        let payload = sse_payload(&RunChartEvent::Equity(ChartEquityPoint {
            time: 1,
            equity_usd: 99.0,
        }))
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["event"], serde_json::json!("equity"));
        assert_eq!(v["data"]["equity_usd"], serde_json::json!(99.0));
    }

    #[test]
    fn status_is_the_terminal_event() {
        assert!(is_terminal(&RunChartEvent::Status {
            phase: "completed".into(),
            message: None
        }));
        assert!(!is_terminal(&RunChartEvent::Equity(ChartEquityPoint {
            time: 0,
            equity_usd: 0.0
        })));
    }
}
