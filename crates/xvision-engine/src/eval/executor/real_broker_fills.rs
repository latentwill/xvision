//! `RealBrokerFills` — Live [`FillSink`] impl. Wraps an
//! `Arc<dyn BrokerSurface>` and translates [`FillRequest`] into
//! `xvision_execution::broker_surface::OrderRequest`, awaits the
//! broker submit, and translates the resulting `OrderConfirmation`
//! into a [`FillRecord`].
//!
//! Sub-track 3 of the 2026-05-21 Alpaca-Live executor refactor
//! (see `team/contracts/live-bar-source-alpaca.md`). Companion to
//! [`crate::eval::executor::SimulatedFills`] — the Backtest path
//! synthesises fills from the next bar's open price; this path
//! sends orders to a real (or paper) broker.
//!
//! ## Error model
//!
//! The [`FillSink`] trait surface is infallible. Real brokers can
//! and do fail; this impl complies with the trait shape by encoding
//! the failure as a no-fill [`FillRecord`] with
//! `order_state = Some(OrderState::Rejected)` and emitting a
//! `tracing::error!` carrying the classified error tag. The caller
//! — eventually the unified `Executor` shell delivered by
//! sub-track 4 — inspects `order_state` and routes the error class
//! through the existing `classify_run_failure` taxonomy (see
//! `eval::executor::classify_run_failure`).
//!
//! The taxonomy itself is owned by
//! [`xvision_execution::broker_surface::classify_broker_error_message`];
//! we call it (rather than re-implementing the matching) so the
//! engine-side and dashboard-side wire shapes never drift.
//!
//! ## No-op handling
//!
//! `action == "hold"` and matching-direction actions
//! (`long_open` while already long, etc.) short-circuit before
//! reaching `submit_order`. This mirrors the early-return in
//! [`crate::eval::executor::traits::simulate_fill_inner`] so the
//! Live + Backtest paths share the same observable shape for the
//! no-op case (`fill_price = None`, `order_state = None`).

use std::sync::Arc;

use async_trait::async_trait;
use xvision_execution::broker_surface::{
    classify_broker_error_message, BrokerErrorClass, BrokerSurface, OrderRequest, Side,
};

use crate::agent::observability::{fresh_span_id, ObsEmitter};
use crate::eval::executor::traits::{FillRecord, FillRequest, FillSink};
use crate::eval::orders::OrderState;
use crate::eval::scenario::FillProvenance;
use xvision_observability::BrokerCallOutcome;

/// Live [`FillSink`] backed by an `Arc<dyn BrokerSurface>`.
///
/// Optionally carries an [`ObsEmitter`] so live fills emit
/// `broker.call` spans on the trace dock — matching the coverage
/// already present on the backtest (simulated fill) path.
pub struct RealBrokerFills {
    broker: Arc<dyn BrokerSurface>,
    obs: Option<ObsEmitter>,
}

impl RealBrokerFills {
    pub fn new(broker: Arc<dyn BrokerSurface>) -> Self {
        Self { broker, obs: None }
    }

    /// Attach an [`ObsEmitter`] so live fills are traced.
    pub fn with_obs(mut self, obs: ObsEmitter) -> Self {
        self.obs = Some(obs);
        self
    }

    /// Whether the wrapped live broker is a directional-perps venue.
    /// Threaded into the engine's R3 veto so perps guards activate only
    /// on perps venues. Spot brokers (Alpaca) return false.
    pub fn is_perp_venue(&self) -> bool {
        self.broker.is_perp_venue()
    }
}

#[async_trait]
impl FillSink for RealBrokerFills {
    async fn submit(&mut self, req: FillRequest) -> FillRecord {
        // 1. No-op detection — same boundary as `simulate_fill_inner`.
        //    A `hold` action never reaches `submit` from a sensible
        //    executor, but we defend in depth to match the trait
        //    contract ("no-op fills return fill_price=None").
        let want_long = req.action == "long_open";
        let want_short = req.action == "short_open";
        let want_flat = !want_long && !want_short;
        if (want_long && req.pos > 0.0) || (want_short && req.pos < 0.0) || (want_flat && req.pos == 0.0) {
            return noop_fill_record(&req);
        }

        // 2. Translate request → OrderRequest.
        //    v1 is market-only. Quantity = risk_pct * equity / next_open
        //    on opens, |pos| on flats (mirroring the sizing in
        //    `simulate_fill_inner`). Side is derived from the action;
        //    `flat` flips against the current pos.
        let trade_long = if want_long {
            true
        } else if want_short {
            false
        } else {
            req.pos < 0.0 // closing a short means buying
        };
        let side = if trade_long { Side::Buy } else { Side::Sell };

        let target_pos = if want_flat {
            0.0
        } else {
            let usd_at_risk = req.equity * req.risk_pct;
            let units = (usd_at_risk / req.next_open).max(0.0);
            if want_long {
                units
            } else {
                -units
            }
        };

        let size = if req.pos == 0.0 {
            target_pos.abs()
        } else if target_pos == 0.0 {
            req.pos.abs()
        } else {
            req.pos.abs() + target_pos.abs()
        };

        if !size.is_finite() || size <= 0.0 {
            // Degenerate sizing — surface as a rejected no-op rather
            // than dispatching a zero-quantity order to the broker.
            tracing::warn!(
                target: "xvision_engine::real_broker_fills",
                asset = %req.asset,
                action = %req.action,
                size,
                "RealBrokerFills: refusing to submit non-positive size"
            );
            return rejected_no_fill(&req, BrokerErrorClass::Unknown, "size_non_positive".into());
        }

        let idempotency_key = format!("live-{}-{}", req.asset, req.bar_ts.timestamp());
        let order = OrderRequest {
            asset: req.asset.clone(),
            side,
            size,
            reference_price_usd: req.next_open,
            stop_loss_pct: None,
            take_profit_pct: None,
            idempotency_key,
        };

        // 3. Submit + translate the outcome.
        // Emit broker.call span if an ObsEmitter is attached, matching
        // the coverage already present on the simulated-fill path.
        //
        // WS-4: the live path also stamps the broker's *real* venue (not
        // the old hardcoded "live") and emits an `order_signed` engine
        // event just before submit, so the trace distinguishes a live
        // fill from a backtest fill (real venue identity + signing path).
        let venue = self.broker.venue().to_string();
        let idempotency_key = order.idempotency_key.clone();
        let broker_span_id = fresh_span_id();
        if let Some(obs) = self.obs.as_ref() {
            let broker_side = if trade_long {
                xvision_observability::BrokerSide::Buy
            } else {
                xvision_observability::BrokerSide::Sell
            };
            obs.emit_broker_call_started(
                &broker_span_id,
                None,
                broker_side,
                req.asset.as_str(),
                size,
                Some(req.next_open),
                "market",
                &venue,
                Some(idempotency_key.clone()),
            )
            .await;

            // `order_signed` — fires BEFORE submit. Carries the venue +
            // signing scheme + order intent, scoped to the broker.call
            // span. NEVER includes keys / secrets / signatures.
            let signed_payload = serde_json::json!({
                "venue": venue,
                "scheme": self.broker.signing_scheme(),
                "asset": req.asset,
                "side": if trade_long { "buy" } else { "sell" },
                "idempotency_key": idempotency_key,
            });
            obs.emit_engine_event(
                "order_signed",
                Some(broker_span_id.clone()),
                Some(signed_payload.to_string()),
            )
            .await;
        }
        match self.broker.submit_order(order).await {
            Ok(conf) => {
                // 4a. Successful fill — translate the confirmation
                //     into a FillRecord. FillProvenance is built
                //     directly from the broker confirmation since
                //     there is no equivalent "from_broker"
                //     constructor on the type today. The slippage /
                //     spread / fee BPS fields are surfaced from the
                //     request (the broker doesn't report them
                //     post-hoc; we record what the executor asked for
                //     so the trace remains consistent with the
                //     pre-submit cost model).
                let fill_price = conf.fill_price.unwrap_or(req.next_open);
                let fill_size = if conf.fill_size > 0.0 {
                    conf.fill_size
                } else {
                    size
                };
                let signed_filled = if trade_long { fill_size } else { -fill_size };
                let new_pos = req.pos + signed_filled;
                let new_entry = if new_pos == 0.0 {
                    0.0
                } else if req.pos != 0.0 && new_pos.signum() == req.pos.signum() {
                    req.entry
                } else {
                    fill_price
                };
                let realized = if req.pos != 0.0 {
                    req.pos * (fill_price - req.entry)
                } else {
                    0.0
                };
                let fee = conf.fee.unwrap_or(0.0);
                let provenance = FillProvenance {
                    slip_bps_applied: req.slip_bps,
                    spread_bps_applied: req.spread_bps,
                    fee_bps_applied: if want_long || want_short {
                        req.taker_bps
                    } else {
                        req.maker_bps
                    },
                    fee_source: req.fee_source,
                    volume_share: 0.0,
                    volume_cap_bound: false,
                };
                // Terminal order state for this fill — computed once so
                // the WS-4 `order_state` engine event and the FillRecord
                // agree byte-for-byte.
                let order_state = if fill_size + f64::EPSILON < size {
                    OrderState::PartiallyFilled
                } else {
                    OrderState::Filled
                };
                if let Some(obs) = self.obs.as_ref() {
                    obs.emit_broker_call_finished(
                        &broker_span_id,
                        BrokerCallOutcome::Filled,
                        Some(fill_price),
                        Some(fill_size),
                        Some(fee),
                        Some(conf.broker_order_id.clone()),
                        None,
                        None,
                        None,
                    )
                    .await;

                    // WS-4 `order_state` — fires after the fill outcome is
                    // known. `state` serializes the `OrderState` to its
                    // snake_case wire form. Scoped to the broker.call span.
                    let state_str = serde_json::to_value(order_state)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_default();
                    let state_payload = serde_json::json!({
                        "asset": req.asset,
                        "state": state_str,
                        "fill_price": fill_price,
                        "fill_size": fill_size,
                        "order_size": size,
                    });
                    obs.emit_engine_event(
                        "order_state",
                        Some(broker_span_id.clone()),
                        Some(state_payload.to_string()),
                    )
                    .await;

                    // WS-4 `venue_account_snapshot` — run-scoped (no span).
                    // Equity is BEST-EFFORT: a balance RPC error must NOT
                    // break the fill or change fill semantics, so we emit
                    // `equity_usd: null` rather than failing. Position is
                    // the post-fill `new_pos` we already computed (no extra
                    // round-trip to the venue).
                    let equity_usd: serde_json::Value = match self.broker.balance().await {
                        Ok(eq) => serde_json::Number::from_f64(eq)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null),
                        Err(e) => {
                            tracing::warn!(
                                target: "xvision_engine::real_broker_fills",
                                venue = %venue,
                                error = %format!("{e:#}"),
                                "venue_account_snapshot: balance RPC failed; emitting null equity"
                            );
                            serde_json::Value::Null
                        }
                    };
                    let snapshot_payload = serde_json::json!({
                        "venue": venue,
                        "position": new_pos,
                        "equity_usd": equity_usd,
                    });
                    obs.emit_engine_event("venue_account_snapshot", None, Some(snapshot_payload.to_string()))
                        .await;
                }
                FillRecord {
                    new_pos,
                    new_entry,
                    fill_price: Some(fill_price),
                    fill_size: Some(fill_size),
                    fee: Some(fee),
                    realized_pnl: realized - fee,
                    provenance,
                    fill_branch: Some(crate::eval::executor::trace_types::FillBranch::NextOpenOnly),
                    aggressor_side: Some(crate::eval::executor::trace_types::AggressorSide::Taker),
                    order_state: Some(order_state),
                    volume_cap_hit: None,
                    broker_error: None,
                }
            }
            Err(e) => {
                // 4b. Broker rejection — classify via the shared
                //     taxonomy and encode the failure as a
                //     `Rejected` no-fill record. The eventual
                //     unified Executor inspects `order_state` and
                //     routes through `classify_run_failure`.
                let msg = format!("{e:#}");
                let class = classify_broker_error_message(&msg);
                if let Some(obs) = self.obs.as_ref() {
                    obs.emit_broker_call_finished(
                        &broker_span_id,
                        BrokerCallOutcome::Rejected,
                        None,
                        None,
                        None,
                        None,
                        Some(class.as_tag().to_string()),
                        Some(msg.clone()),
                        Some("warn"),
                    )
                    .await;
                }
                tracing::error!(
                    target: "xvision_engine::real_broker_fills",
                    asset = %req.asset,
                    action = %req.action,
                    error_class = class.as_tag(),
                    error_message = %msg,
                    "RealBrokerFills: broker rejected order"
                );
                rejected_no_fill(&req, class, msg)
            }
        }
    }
}

/// Build a "no-op" `FillRecord` for cases where the action is a hold
/// or matches the current position. Mirrors the no-op branch in
/// `simulate_fill_inner` byte-for-byte (modulo the inputs we have
/// access to here).
fn noop_fill_record(req: &FillRequest) -> FillRecord {
    FillRecord {
        new_pos: req.pos,
        new_entry: req.entry,
        fill_price: None,
        fill_size: None,
        fee: None,
        realized_pnl: 0.0,
        provenance: FillProvenance::default(),
        fill_branch: None,
        aggressor_side: None,
        order_state: None,
        volume_cap_hit: None,
        broker_error: None,
    }
}

/// Build a "broker rejected" no-fill `FillRecord`. `class_tag` is
/// the snake-case error class from
/// `BrokerErrorClass::as_tag` (or a custom tag for non-broker
/// rejections like sizing degeneracies).
fn rejected_no_fill(req: &FillRequest, class: BrokerErrorClass, reason: String) -> FillRecord {
    FillRecord {
        new_pos: req.pos,
        new_entry: req.entry,
        fill_price: None,
        fill_size: None,
        fee: None,
        realized_pnl: 0.0,
        provenance: FillProvenance::default(),
        fill_branch: None,
        aggressor_side: None,
        order_state: Some(OrderState::Rejected),
        volume_cap_hit: None,
        broker_error: Some((class, reason)),
    }
}
