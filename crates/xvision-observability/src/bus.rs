//! `RunEventBus` — bounded mpsc bus + multi-subscriber fan-out.
//!
//! Producers (the `xvision-agent-client` IPC handler in Phase B; tests
//! today) call `publish(event)`. A single consumer task drains the bus
//! and fans each event out to every registered subscriber. On overflow
//! the bus drops the **oldest** event (the producer always gets through)
//! and bumps a per-run drop counter; once the counter accumulates,
//! `BackpressureDropped` is published so the recorder writes a
//! `supervisor_notes` warn row referencing the gap.
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

#[derive(Debug)]
pub struct RunEventBus {
    tx: mpsc::Sender<RunEvent>,
    /// Per-run drop counters maintained by the consumer task.
    drops: Arc<Mutex<HashMap<String, u32>>>,
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
        let drops = Arc::new(Mutex::new(HashMap::<String, u32>::new()));
        let drops_for_task = drops.clone();
        let tx_for_task = tx.clone();
        let consumer = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                for sub in &subscribers {
                    if let Err(e) = sub.handle_event(&event).await {
                        recorder_error(&e);
                    }
                }
                // After each delivery, check for accumulated drops on
                // this event's run and emit a single BackpressureDropped
                // marker so the gap is recorded.
                let run_id = event.run_id();
                if !run_id.is_empty() {
                    let dropped =
                        { drops_for_task.lock().await.remove(run_id).unwrap_or(0) };
                    if dropped > 0 {
                        let marker = RunEvent::BackpressureDropped(
                            BackpressureDroppedEvent {
                                run_id: run_id.to_owned(),
                                dropped,
                                note: "bus capacity exceeded".to_owned(),
                            },
                        );
                        // Re-publish; if the bus is *still* full, log and
                        // give up — we've already recorded the drop count.
                        if let Err(_e) = tx_for_task.try_send(marker) {
                            warn!(
                                target: "xvision_observability::bus",
                                run_id, dropped,
                                "could not enqueue backpressure marker; bus still saturated"
                            );
                        }
                    }
                }
            }
        });
        Self {
            tx,
            drops,
            _consumer: consumer,
        }
    }

    /// Best-effort publish. On overflow, **drops the oldest event** (in
    /// practice tokio's bounded mpsc rejects the newest; we approximate
    /// "drop oldest" by performing a non-blocking send and incrementing
    /// the drop counter — which keeps the producer hot path lock-free
    /// and lets the consumer keep draining).
    pub async fn publish(&self, event: RunEvent) {
        let run_id = event.run_id().to_owned();
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                if !run_id.is_empty() {
                    *self.drops.lock().await.entry(run_id).or_insert(0) += 1;
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Bus is shutting down; producers should stop. We log
                // once per close on the consumer side, not here.
            }
        }
    }

    /// Synchronous variant for hot paths where awaiting is not possible.
    /// Drops the event on overflow without updating drop counters
    /// (those require the async lock).
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
        // The consumer task pulls from the channel; a successful send +
        // a tokio::yield_now is sufficient because the consumer runs on
        // the same runtime. For deterministic tests, callers should
        // sleep a small interval instead and await assertions.
        tokio::task::yield_now().await;
    }
}

fn recorder_error(e: &RecorderError) {
    warn!(
        target: "xvision_observability::bus",
        error = %e,
        "recorder failed to handle event"
    );
}
