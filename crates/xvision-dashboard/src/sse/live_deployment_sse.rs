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
//! the stream delivers equity ticks (`RunChartEvent::Equity` → `event: metrics`,
//! equity-only) and lifecycle/terminal `status` frames. The widened CT5 capital
//! block (`deployed_capital_usd`, `unrealized_pnl_usd`, `realized_pnl_usd`,
//! `daily_loss_limit_remaining_usd`, `drawdown_pct`) and `risk_veto` are emitted
//! on the engine `ProgressBus` but are NOT yet projected onto the
//! `RunChartEvent` the SSE reads — consumers read the full capital metrics block
//! via the 5s poll endpoint (`GET /api/live/deployments`). Per-tick capital
//! streaming is a DEFERRED follow-up (it requires widening `RunChartEvent`,
//! explicitly out of scope here).
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
/// NOTE on `metrics`: the SSE `metrics` frame carries the `RunChartEvent::Equity`
/// payload, which is EQUITY ONLY (`{ time, equity_usd }`). The full CT5 capital
/// block is NOT on this frame — it rides the engine `ProgressBus` and is read by
/// consumers via the 5s poll endpoint. Per-tick capital streaming (widening
/// `RunChartEvent`) is a deferred follow-up.
pub fn event_name(ev: &RunChartEvent) -> &'static str {
    match ev {
        RunChartEvent::Bar(_) => "bar",
        RunChartEvent::IndicatorTail(_) => "indicator_tail",
        RunChartEvent::Decision(_) => "decision",
        RunChartEvent::Marker(_) => "marker",
        RunChartEvent::Equity(_) => "metrics",
        RunChartEvent::Status { .. } => "status",
    }
}

/// True for the stream-closing lifecycle event. A live deployment's stream ends
/// when the run emits its terminal `Status` (the executor drops the channel
/// after this), matching `eval_runs::stream`.
fn is_terminal(ev: &RunChartEvent) -> bool {
    matches!(ev, RunChartEvent::Status { .. })
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
                    match serde_json::to_string(&ev) {
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
    use xvision_engine::api::chart::{ChartEquityPoint, RunChartEvent};

    #[test]
    fn event_names_match_run_chart_variants() {
        assert_eq!(
            event_name(&RunChartEvent::Equity(ChartEquityPoint {
                time: 0,
                equity_usd: 0.0
            })),
            "metrics"
        );
        assert_eq!(
            event_name(&RunChartEvent::Status {
                phase: "running".into(),
                message: None
            }),
            "status"
        );
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
