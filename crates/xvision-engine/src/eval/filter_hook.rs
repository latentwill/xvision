//! Per-bar filter hook for the backtest executor.
//!
//! Stage 2 of the Filter v1 plan. Stitches the engine-independent
//! [`xvision_filters::RuntimeFilter`] into a backtest's per-bar loop.
//!
//! Two responsibilities:
//!
//! 1. Adapt `xvision_core::market::Ohlcv` into the runtime's local
//!    [`xvision_filters::Bar`] reduction.
//! 2. Persist each bar's evaluation to `eval_filter_evaluations` so
//!    the "plan-touch" ledger is reconstructable post-run, and emit a
//!    matching `ProgressEvent::FilterEvaluated`.
//!
//! Construction is gated on `Strategy.activation_mode == FilterGated`
//! via [`FilterHook::new`]; for `EveryBar` strategies (the default) the
//! hook is `None` and the executor's per-bar loop short-circuits.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use xvision_core::market::Ohlcv;
use xvision_filters::{
    runtime::{ActivationDecision, EvalContext, FilterEvalOutcome, RuntimeFilter},
    Bar, Filter, FilterEventV1, FilterState,
};

use crate::agent::observability::ObsEmitter;
use crate::eval::progress::{send_event, ProgressEvent, ProgressTx};
use crate::strategies::Strategy;

/// Per-run filter runtime + persistence glue.
///
/// Lifetime is the eval run: built at the top of the executor's
/// `run_inner`, evaluated on every bar of the per-bar loop, dropped at
/// run end. Holds no DB connection of its own; the executor passes the
/// pool by reference on each evaluation so the hook is `Send + Sync`
/// and safe to keep alive across awaits.
pub struct FilterHook {
    filter: Filter,
    state: FilterState,
    /// Cached display name copy so a per-bar insert doesn't need to
    /// read `filter.display_name` through the borrow.
    display_name: String,
    bar_index: u64,
    /// Observability emitter for the eval run, threaded in via
    /// [`FilterHook::with_obs`]. `None` for the CLI / unit-test path
    /// where the bus isn't wired — then `record` is a no-op on the obs
    /// side and only writes the `eval_filter_evaluations` ledger row +
    /// the `ProgressEvent::FilterEvaluated`. When `Some`, a *trip* (the
    /// FIRE/wake event) additionally emits a `filter_fired` engine event
    /// so the trace dock can render filter firings alongside the rest of
    /// the per-decision actions.
    obs: Option<ObsEmitter>,
}

/// One per-bar evaluation result plus the public event shape persisted
/// for export/API consumers.
#[derive(Debug, Clone)]
pub struct FilterEvaluationRecord {
    pub outcome: FilterEvalOutcome,
    pub event: FilterEventV1,
    pub trigger_context: Option<serde_json::Value>,
}

impl FilterHook {
    /// Returns `Some` only when `strategy.activation_mode == FilterGated`
    /// AND `strategy.filter` is present. Returns `None` for `EveryBar`
    /// strategies (the default) — the executor's loop should treat
    /// `None` as "no gating, run pipeline on every bar as before".
    ///
    /// `CompiledRules` is rejected with an error — that mode is reserved
    /// for v1.5 and the runtime cannot honor it.
    pub fn new(strategy: &Strategy) -> anyhow::Result<Option<Self>> {
        use xvision_filters::ActivationMode;
        match strategy.activation_mode {
            ActivationMode::EveryBar => Ok(None),
            ActivationMode::CompiledRules => Err(anyhow::anyhow!(
                "E_FILTER_ACTIVATION_MODE_NOT_IMPL: `CompiledRules` activation mode is reserved for v1.5"
            )),
            ActivationMode::FilterGated => {
                let filter = strategy.filter.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "E_FILTER_GATED_WITHOUT_FILTER: strategy.activation_mode = FilterGated but strategy.filter is None"
                    )
                })?;
                // Validate once at run start; surfaces parse errors clearly.
                xvision_filters::validate(filter)?;
                let state = FilterState::new(filter);
                let display_name = filter.display_name.clone();
                Ok(Some(Self {
                    filter: filter.clone(),
                    state,
                    display_name,
                    bar_index: 0,
                    obs: None,
                }))
            }
        }
    }

    /// Attach the eval run's [`ObsEmitter`] so a filter trip emits a
    /// `filter_fired` engine event onto the observability bus. The
    /// executor wires this from its own `obs_emitter` at hook-build
    /// time. Passing `None` (or never calling this) keeps the hook on
    /// the legacy table-only path. Builder style mirrors the rest of
    /// the obs surface (`with_observability`, `with_retention`).
    pub fn with_obs(mut self, obs: Option<ObsEmitter>) -> Self {
        self.obs = obs;
        self
    }

    /// Evaluate one bar. Returns the outcome so the executor can decide
    /// whether to skip the agent pipeline.
    pub fn evaluate(&mut self, bar: &Ohlcv, in_position: bool) -> FilterEvaluationRecord {
        let runtime = RuntimeFilter::from_validated(&self.filter);
        let local_bar =
            Bar::with_timestamp(bar.open, bar.high, bar.low, bar.close, bar.volume, bar.timestamp);
        let ctx = EvalContext {
            ts: bar.timestamp,
            in_position,
        };
        let outcome = runtime.evaluate(&mut self.state, &local_bar, ctx);
        let event = FilterEventV1::from_outcome(
            self.filter.id.clone(),
            bar.timestamp,
            &outcome,
            self.state.indicator_snapshot(&self.filter),
        );
        let trigger_context = outcome
            .decision
            .is_active()
            .then(|| self.build_trigger_context(&event.indicator_snapshot))
            .flatten();
        self.bar_index += 1;
        FilterEvaluationRecord {
            outcome,
            event,
            trigger_context,
        }
    }

    fn build_trigger_context(
        &self,
        indicator_snapshot: &std::collections::BTreeMap<String, f64>,
    ) -> Option<serde_json::Value> {
        let fire = self.filter.fire.as_ref()?;
        let mut values = serde_json::Map::new();
        for indicator in &fire.context {
            let token = indicator.to_string();
            if let Some(value) = indicator_snapshot.get(&token) {
                values.insert(token, serde_json::json!(value));
            }
        }
        Some(serde_json::json!({
            "reason": fire.reason,
            "priority": fire.priority,
            "tags": fire.tags,
            "context": values,
        }))
    }

    /// Persist a row to `eval_filter_evaluations` and emit the matching
    /// `ProgressEvent::FilterEvaluated`. Called once per bar after
    /// [`Self::evaluate`].
    pub async fn record(
        &self,
        pool: &SqlitePool,
        progress_tx: Option<&ProgressTx>,
        run_id: &str,
        ts: DateTime<Utc>,
        evaluation: &FilterEvaluationRecord,
    ) -> anyhow::Result<()> {
        let outcome = &evaluation.outcome;
        let decision_json = serde_json::to_string(&outcome.decision)?;
        let event_json = serde_json::to_string(&evaluation.event)?;
        let conditions_json = serde_json::to_string(
            &outcome
                .conditions_passed
                .iter()
                .map(|c| c.passed)
                .collect::<Vec<_>>(),
        )?;
        let in_warmup = matches!(outcome.decision, ActivationDecision::Warming { .. }) as i64;
        let in_cooldown = matches!(outcome.decision, ActivationDecision::Cooldown { .. }) as i64;
        let wakeups_today: i64 = match outcome.decision {
            ActivationDecision::CappedForDay { wakeups_today } => wakeups_today as i64,
            _ => self.state.wakeups_on(ts) as i64,
        };
        let bar_index_i = (self.bar_index.saturating_sub(1)) as i64;

        sqlx::query(
            "INSERT INTO eval_filter_evaluations \
             (run_id, bar_index, ts, filter_id, filter_display_name, \
              decision_tag, decision_json, conditions_passed, tree_true, \
              in_warmup, in_cooldown, wakeups_today, filter_event_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(bar_index_i)
        .bind(ts.to_rfc3339())
        .bind(self.filter.id.as_str())
        .bind(&self.display_name)
        .bind(outcome.decision.tag())
        .bind(&decision_json)
        .bind(&conditions_json)
        .bind(outcome.tree_true as i64)
        .bind(in_warmup)
        .bind(in_cooldown)
        .bind(wakeups_today)
        .bind(&event_json)
        .execute(pool)
        .await?;

        if let Some(tx) = progress_tx {
            send_event(
                tx,
                ProgressEvent::FilterEvaluated {
                    run_id: run_id.to_string(),
                    bar_index: self.bar_index.saturating_sub(1),
                    ts,
                    decision_tag: outcome.decision.tag().to_string(),
                    conditions_passed: outcome.conditions_passed.iter().map(|c| c.passed).collect(),
                    tree_true: outcome.tree_true,
                    trip: outcome.decision.is_trip(),
                },
            );
        }

        // trace-obs WS-6: surface the deterministic FIRE/wake on the
        // observability bus IN ADDITION to the ledger row + ProgressEvent
        // above, so the trace dock renders a filter firing like any other
        // engine action. Emit ONLY on a trip — the `eval_filter_evaluations`
        // table already carries per-bar detail, so a bus event every bar
        // would be noise. Run-scoped (`span_id = None`) to match the other
        // bar-level engine events the executor emits (early_stop_triggered,
        // risk_veto, …). No-op when no emitter is wired.
        if outcome.decision.is_trip() {
            if let Some(obs) = self.obs.as_ref() {
                let reason = self
                    .filter
                    .fire
                    .as_ref()
                    .map(|f| serde_json::Value::String(f.reason.clone()))
                    .unwrap_or(serde_json::Value::Null);
                let payload = serde_json::json!({
                    "filter_id": self.filter.id.as_str(),
                    "rule": self.display_name,
                    "decision_index": bar_index_i,
                    "outcome": outcome.decision.tag(),
                    "reason": reason,
                });
                obs.emit_engine_event("filter_fired", None, Some(payload.to_string()))
                    .await;
            }
        }

        Ok(())
    }
}
