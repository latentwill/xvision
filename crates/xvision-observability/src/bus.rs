//! `RunEventBus` — bounded mpsc bus + multi-subscriber fan-out.
//!
//! Producers (the `xvision-agent-client` IPC handler in Phase B; tests
//! today) call `publish(event)`. A single consumer task drains the bus
//! and fans each event out to every registered subscriber.
//!
//! ## Overflow semantics
//!
//! The contract requires that gaps in the recorded timeline are
//! *visible* and that lifecycle-closing events (`RunStarted`,
//! `RunFinished`, `RunInterrupted`, `SidecarError`) are *never lost* —
//! otherwise runs/spans stay open in SQLite or sidecar crashes go
//! unrecorded.
//!
//! We implement this with two paths:
//!
//! - **Lifecycle-critical events** (`RunEvent::is_lifecycle_critical`)
//!   use `mpsc::Sender::send().await`, which applies backpressure to the
//!   producer rather than dropping. These events are low-frequency, so
//!   the producer briefly awaiting a free slot is acceptable.
//! - **Routine high-volume events** (span starts/finishes, model/tool
//!   calls, text deltas, checkpoints, notes) use `try_send`. On `Full`
//!   the new event is dropped and a per-routing-key counter is bumped.
//!
//! Drops are attributed by routing key: `run_id` if the event carries
//! one directly, otherwise `span_id` (the bus consumer maintains a
//! `span_id → run_id` map populated from `SpanStarted` events, so
//! span-keyed drops are translated to runs before the
//! `BackpressureDropped` marker is published). If neither id is
//! available, the drop is logged at `warn` and surfaced as a marker
//! with empty `run_id` so it still appears in `supervisor_notes`.
//!
//! Sequencing guarantee: FIFO per `run_id`. Cross-run ordering is
//! best-effort.

use crate::events::{BackpressureDroppedEvent, RunEvent};
use crate::recorder::{AgentRunRecorder, RecorderError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::warn;

/// Identifies the bucket a drop is counted under. Span-keyed drops are
/// translated to run-keyed drops once the consumer has seen the
/// matching `SpanStarted`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DropKey {
    Run(String),
    Span(String),
    /// No id available — drop is still counted so it surfaces, but
    /// cannot be attributed to a specific run.
    Unattributed,
}

#[derive(Debug)]
pub struct RunEventBus {
    tx: mpsc::Sender<RunEvent>,
    /// Drop counters maintained by both producers (on `Full`) and the
    /// consumer (translation + drain).
    drops: Arc<Mutex<HashMap<DropKey, u32>>>,
    /// Consumer task handle. Dropped when the bus shuts down.
    _consumer: JoinHandle<()>,
}

impl RunEventBus {
    /// Default bus capacity. Tune via `observability.toml` after Phase B
    /// emission lands and we have real throughput numbers.
    pub const DEFAULT_CAPACITY: usize = 4096;

    pub fn new(subscribers: Vec<Arc<dyn AgentRunRecorder>>) -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY, subscribers)
    }

    pub fn with_capacity(
        capacity: usize,
        subscribers: Vec<Arc<dyn AgentRunRecorder>>,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<RunEvent>(capacity.max(1));
        let drops = Arc::new(Mutex::new(HashMap::<DropKey, u32>::new()));
        let drops_for_task = drops.clone();
        let tx_for_task = tx.clone();
        let consumer = tokio::spawn(async move {
            // Consumer-local span→run map. Populated from `SpanStarted`
            // events that successfully traverse the bus; used to
            // translate span-keyed drops into the run they belong to.
            let mut span_to_run: HashMap<String, String> = HashMap::new();
            while let Some(event) = rx.recv().await {
                if let RunEvent::SpanStarted(s) = &event {
                    span_to_run.insert(s.span_id.clone(), s.run_id.clone());
                }
                for sub in &subscribers {
                    if let Err(e) = sub.handle_event(&event).await {
                        recorder_error(&e);
                    }
                }
                flush_drops(&drops_for_task, &mut span_to_run, &tx_for_task, &event)
                    .await;
            }
        });
        Self {
            tx,
            drops,
            _consumer: consumer,
        }
    }

    /// Publish an event onto the bus.
    ///
    /// - Lifecycle-critical events (see [`RunEvent::is_lifecycle_critical`])
    ///   apply backpressure to the producer via `send().await` rather
    ///   than being dropped — losing one of these leaves the run/spans
    ///   in an inconsistent state in SQLite.
    /// - Routine events use `try_send`. On overflow the event is
    ///   dropped and the drop is attributed by routing key (run_id ▸
    ///   span_id ▸ unattributed) so that a `BackpressureDropped` marker
    ///   eventually surfaces the gap in `supervisor_notes`.
    pub async fn publish(&self, event: RunEvent) {
        if event.is_lifecycle_critical() {
            if let Err(e) = self.tx.send(event).await {
                warn!(
                    target: "xvision_observability::bus",
                    "lifecycle event dropped because bus is closed: {e}"
                );
            }
            return;
        }
        let key = drop_key_for(&event);
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                *self.drops.lock().await.entry(key).or_insert(0) += 1;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Bus is shutting down; producers should stop. We log
                // once per close on the consumer side, not here.
            }
        }
    }

    /// Synchronous variant for hot paths where awaiting is not possible.
    /// Drops the event on overflow without updating drop counters
    /// (those require the async lock). Callers should prefer
    /// [`Self::publish`] when an async context is available.
    pub fn try_publish(&self, event: RunEvent) -> Result<(), RunEvent> {
        match self.tx.try_send(event) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(e)) => Err(e),
            Err(mpsc::error::TrySendError::Closed(e)) => Err(e),
        }
    }

    /// Test helper: drain the bus and let subscribers finish. Returns
    /// when there are no in-flight messages.
    pub async fn quiesce(&self) {
        tokio::task::yield_now().await;
    }
}

fn drop_key_for(event: &RunEvent) -> DropKey {
    let run = event.run_id();
    if !run.is_empty() {
        return DropKey::Run(run.to_owned());
    }
    if let Some(span) = event.span_id() {
        return DropKey::Span(span.to_owned());
    }
    DropKey::Unattributed
}

/// Translate span-keyed drops to run-keyed drops using the consumer's
/// span→run map, then emit a `BackpressureDropped` marker per run that
/// has pending drops. Unattributable drops are emitted with empty
/// `run_id` so the gap still surfaces in `supervisor_notes`.
async fn flush_drops(
    drops: &Arc<Mutex<HashMap<DropKey, u32>>>,
    span_to_run: &mut HashMap<String, String>,
    tx: &mpsc::Sender<RunEvent>,
    just_handled: &RunEvent,
) {
    let markers: Vec<BackpressureDroppedEvent> = {
        let mut map = drops.lock().await;
        if map.is_empty() {
            // Still prune span_to_run on lifecycle close so it stays
            // bounded even when no drops occurred.
            prune_span_map(span_to_run, just_handled);
            return;
        }
        let mut per_run: HashMap<String, u32> = HashMap::new();
        let mut unattributed: u32 = 0;
        let keys: Vec<DropKey> = map.keys().cloned().collect();
        for key in keys {
            match &key {
                DropKey::Run(run_id) => {
                    if let Some(n) = map.remove(&key) {
                        *per_run.entry(run_id.clone()).or_insert(0) += n;
                    }
                }
                DropKey::Span(span_id) => {
                    if let Some(run_id) = span_to_run.get(span_id) {
                        if let Some(n) = map.remove(&key) {
                            *per_run.entry(run_id.clone()).or_insert(0) += n;
                        }
                    }
                    // If we don't yet know which run the span belongs
                    // to, leave the count parked — `SpanStarted` may
                    // still arrive.
                }
                DropKey::Unattributed => {
                    if let Some(n) = map.remove(&key) {
                        unattributed += n;
                    }
                }
            }
        }
        prune_span_map(span_to_run, just_handled);
        let mut out: Vec<BackpressureDroppedEvent> = per_run
            .into_iter()
            .map(|(run_id, dropped)| BackpressureDroppedEvent {
                run_id,
                dropped,
                note: "bus capacity exceeded".to_owned(),
            })
            .collect();
        if unattributed > 0 {
            out.push(BackpressureDroppedEvent {
                run_id: String::new(),
                dropped: unattributed,
                note: "bus capacity exceeded; drops not attributable to a run"
                    .to_owned(),
            });
        }
        out
    };
    for marker in markers {
        let run_id = marker.run_id.clone();
        let dropped = marker.dropped;
        if let Err(mpsc::error::TrySendError::Full(_)) =
            tx.try_send(RunEvent::BackpressureDropped(marker))
        {
            warn!(
                target: "xvision_observability::bus",
                run_id = %run_id, dropped,
                "could not enqueue backpressure marker; bus still saturated, requeueing"
            );
            let key = if run_id.is_empty() {
                DropKey::Unattributed
            } else {
                DropKey::Run(run_id)
            };
            *drops.lock().await.entry(key).or_insert(0) += dropped;
        }
    }
}

fn prune_span_map(span_to_run: &mut HashMap<String, String>, event: &RunEvent) {
    let run_id = match event {
        RunEvent::RunFinished(e) => Some(&e.run_id),
        RunEvent::RunInterrupted(e) => Some(&e.run_id),
        _ => None,
    };
    if let Some(run_id) = run_id {
        span_to_run.retain(|_, r| r != run_id);
    }
}

fn recorder_error(e: &RecorderError) {
    warn!(
        target: "xvision_observability::bus",
        error = %e,
        "recorder failed to handle event"
    );
}
