//! `RunEventBus` — bounded ring-buffer bus + multi-subscriber fan-out.
//!
//! Producers (the `xvision-agent-client` IPC handler in Phase B; tests
//! today) call `publish(event)`. A single consumer task drains the bus
//! and fans each event out to every registered subscriber.
//!
//! ## Overflow semantics
//!
//! Per the Phase A contract, the bus **drops the oldest queued event on
//! full**, increments a per-routing-key drop counter, and emits a
//! `BackpressureDropped` event so a downstream `supervisor_notes` row
//! records the gap.
//!
//! Lifecycle-closing events (`RunStarted`, `RunFinished`,
//! `RunInterrupted`, `SidecarError`) must never be lost — otherwise
//! runs/spans stay open in SQLite or sidecar crashes go unrecorded.
//! The eviction scan skips them: on full we drop the oldest
//! **non-lifecycle** event. In the degenerate case where every queued
//! event is lifecycle-critical, the producer is awaited until the
//! consumer drains a slot (true backpressure). That path requires
//! sustained sidecar-crash-level event rates and is not expected in
//! practice.
//!
//! Drops are attributed by routing key against the EVICTED event:
//! `run_id` if it carries one directly, otherwise `span_id` (the bus
//! consumer maintains a `span_id → run_id` map populated from
//! `SpanStarted` events, so span-keyed drops are translated to runs
//! before the `BackpressureDropped` marker is published). If neither id
//! is available, the drop surfaces as an unattributed marker.
//!
//! Sequencing guarantee: FIFO per `run_id`. Cross-run ordering is
//! best-effort.

use crate::events::{BackpressureDroppedEvent, RunEvent};
use crate::recorder::{AgentRunRecorder, RecorderError};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tracing::warn;

/// Internal: outcome of one publish attempt. `Blocked` carries the
/// event back so the caller can retry without cloning.
enum PublishOutcome {
    Evicted(RunEvent),
    Blocked(RunEvent),
}

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

struct Inner {
    capacity: usize,
    queue: Mutex<VecDeque<RunEvent>>,
    drops: Mutex<HashMap<DropKey, u32>>,
    /// `span_id → run_id` map, populated when a `SpanStarted` is
    /// **published** (not when consumed). Producer-side population
    /// matters because a `SpanStarted` can be evicted on full before
    /// the consumer ever sees it — without producer-side population,
    /// subsequent span-scoped drops for that span would never
    /// translate to a run.
    span_to_run: Mutex<HashMap<String, String>>,
    /// Wakes the consumer when a new event arrives.
    notify_consumer: Notify,
    /// Wakes a backpressured producer when the consumer drains a slot.
    /// Only used in the degenerate "queue full of lifecycle events"
    /// fallback.
    notify_producer: Notify,
    /// Monotone count of events that have been ENQUEUED onto the bus
    /// (every successful `push_back`, including re-queued backpressure
    /// markers). Paired with `settled` to give `quiesce` an
    /// unambiguous, race-free drain signal: snapshot `enqueued` on
    /// entry, then wait until `settled` catches up.
    enqueued: AtomicU64,
    /// Monotone count of events that have LEFT the bus for good — either
    /// fully handled by every subscriber (the consumer finished
    /// `handle_event`) or evicted on overflow. `settled >= enqueued`
    /// means nothing is queued and nothing is mid-fan-out.
    settled: AtomicU64,
    /// Woken each time `settled` advances, so a parked `quiesce`
    /// re-checks its `settled >= snapshot` condition.
    notify_quiesce: Notify,
    closed: AtomicBool,
}

impl Inner {
    /// Record that one event has permanently left the bus (handled or
    /// evicted) and wake any parked `quiesce`.
    fn settle_one(&self) {
        self.settled.fetch_add(1, Ordering::Release);
        self.notify_quiesce.notify_waiters();
    }
}

pub struct RunEventBus {
    inner: Arc<Inner>,
    _consumer: JoinHandle<()>,
}

impl std::fmt::Debug for RunEventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunEventBus")
            .field("capacity", &self.inner.capacity)
            .finish()
    }
}

impl Drop for RunEventBus {
    fn drop(&mut self) {
        // Tell the consumer to exit so background tasks don't outlive
        // the bus handle. The consumer wakes on either `notify_consumer`
        // or the `closed` flag below.
        self.inner.closed.store(true, Ordering::Release);
        self.inner.notify_consumer.notify_waiters();
    }
}

impl RunEventBus {
    /// Default bus capacity. Tune via `observability.toml` after Phase B
    /// emission lands and we have real throughput numbers.
    pub const DEFAULT_CAPACITY: usize = 4096;

    pub fn new(subscribers: Vec<Arc<dyn AgentRunRecorder>>) -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY, subscribers)
    }

    pub fn with_capacity(capacity: usize, subscribers: Vec<Arc<dyn AgentRunRecorder>>) -> Self {
        let inner = Arc::new(Inner {
            capacity: capacity.max(1),
            queue: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
            drops: Mutex::new(HashMap::new()),
            span_to_run: Mutex::new(HashMap::new()),
            notify_consumer: Notify::new(),
            notify_producer: Notify::new(),
            enqueued: AtomicU64::new(0),
            settled: AtomicU64::new(0),
            notify_quiesce: Notify::new(),
            closed: AtomicBool::new(false),
        });
        let inner_for_task = inner.clone();
        let consumer = tokio::spawn(async move {
            consumer_loop(inner_for_task, subscribers).await;
        });
        Self {
            inner,
            _consumer: consumer,
        }
    }

    /// Publish an event onto the bus.
    ///
    /// On full, the oldest non-lifecycle event is evicted to make room
    /// and a drop counter is incremented (attributed to the evicted
    /// event's run). If every queued event is lifecycle-critical, the
    /// producer is awaited until the consumer drains a slot — this is
    /// the only backpressure path and exists so lifecycle markers are
    /// never lost.
    pub async fn publish(&self, event: RunEvent) {
        // Populate the bus-wide span→run map BEFORE we attempt to
        // enqueue. The event itself may be evicted on full, but the
        // mapping survives so future span-scoped drops can still be
        // attributed to the right run.
        if let RunEvent::SpanStarted(s) = &event {
            self.inner
                .span_to_run
                .lock()
                .await
                .insert(s.span_id.clone(), s.run_id.clone());
        }
        let mut pending = event;
        loop {
            let outcome = {
                let mut q = self.inner.queue.lock().await;
                if q.len() < self.inner.capacity {
                    q.push_back(pending);
                    self.inner.enqueued.fetch_add(1, Ordering::Release);
                    self.inner.notify_consumer.notify_one();
                    return;
                }
                if let Some(idx) = q.iter().position(|e| !e.is_lifecycle_critical()) {
                    let evicted = q.remove(idx).expect("idx was just observed");
                    q.push_back(pending);
                    // Net queue size unchanged: the new event is enqueued and
                    // the evicted one settles (it will never be handled).
                    self.inner.enqueued.fetch_add(1, Ordering::Release);
                    self.inner.settle_one();
                    PublishOutcome::Evicted(evicted)
                } else {
                    // Every queued event is lifecycle-critical — we
                    // can't drop any of them. Hand `pending` back so
                    // we can retry after the consumer drains a slot.
                    PublishOutcome::Blocked(pending)
                }
            };
            match outcome {
                PublishOutcome::Evicted(e) => {
                    let key = drop_key_for(&e);
                    *self.inner.drops.lock().await.entry(key).or_insert(0) += 1;
                    self.inner.notify_consumer.notify_one();
                    return;
                }
                PublishOutcome::Blocked(returned) => {
                    pending = returned;
                    self.inner.notify_producer.notified().await;
                    continue;
                }
            }
        }
    }

    /// Synchronous variant for hot paths where awaiting is not possible.
    /// On full, the oldest non-lifecycle event is evicted; returns
    /// `Err(event)` only when the bus is closed or no non-lifecycle
    /// event can be evicted. Callers should prefer [`Self::publish`]
    /// when an async context is available because the sync path can
    /// only make best-effort drop accounting (it cannot await the
    /// async drops-map lock).
    #[allow(clippy::result_large_err)] // RunEvent is intentionally large; boxing changes the caller API
    pub fn try_publish(&self, event: RunEvent) -> Result<(), RunEvent> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(event);
        }
        let mut q = match self.inner.queue.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(event),
        };
        if q.len() < self.inner.capacity {
            q.push_back(event);
            self.inner.enqueued.fetch_add(1, Ordering::Release);
            self.inner.notify_consumer.notify_one();
            return Ok(());
        }
        if let Some(idx) = q.iter().position(|e| !e.is_lifecycle_critical()) {
            let evicted = q.remove(idx).expect("idx was just observed");
            q.push_back(event);
            self.inner.enqueued.fetch_add(1, Ordering::Release);
            self.inner.settle_one();
            drop(q);
            // Best-effort drop accounting without awaiting.
            if let Ok(mut drops) = self.inner.drops.try_lock() {
                *drops.entry(drop_key_for(&evicted)).or_insert(0) += 1;
            }
            self.inner.notify_consumer.notify_one();
            return Ok(());
        }
        Err(event)
    }

    /// Block until every event published *before this call* has fully
    /// settled — handled by every subscriber (the consumer finished
    /// `handle_event`, so the recorder INSERT has committed) or evicted on
    /// overflow. Used by short-lived producers (the `xvn eval run` CLI
    /// process) to guarantee the recorder has persisted everything before
    /// the process exits; without it the async consumer task can be
    /// dropped with events still queued, losing the trace.
    ///
    /// Implemented on a `enqueued`/`settled` sequence pair rather than a
    /// queue-empty + processing-flag check: the latter has a window where
    /// the consumer has popped the last event but not yet marked itself
    /// busy, letting `quiesce` return one event early. Snapshotting
    /// `enqueued` and waiting for `settled` to catch up is race-free and
    /// independent of consumer scheduling.
    ///
    /// Events published concurrently *after* `quiesce` is entered are not
    /// guaranteed to be drained — callers should `quiesce` only once the
    /// run has finished publishing (after `emit_run_finished`).
    pub async fn quiesce(&self) {
        // Snapshot the high-water mark of enqueued events. We only promise
        // to drain everything up to here.
        let target = self.inner.enqueued.load(Ordering::Acquire);
        loop {
            // Register interest BEFORE the check so we can't miss a wake
            // that fires between the check and the await.
            let woken = self.inner.notify_quiesce.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();

            if self.inner.settled.load(Ordering::Acquire) >= target
                || self.inner.closed.load(Ordering::Acquire)
            {
                return;
            }
            // Wait for the consumer to settle another event, with a short
            // timeout as a safety net against a lost wake. Re-checks on loop.
            let _ = tokio::time::timeout(std::time::Duration::from_millis(50), woken).await;
        }
    }
}

async fn consumer_loop(inner: Arc<Inner>, subscribers: Vec<Arc<dyn AgentRunRecorder>>) {
    loop {
        let event = match next_event(&inner).await {
            Some(e) => e,
            None => return,
        };
        for sub in &subscribers {
            if let Err(e) = sub.handle_event(&event).await {
                recorder_error(&e);
            }
        }
        flush_drops(&inner, &event).await;
        // This event is now fully handled (the recorder INSERT has
        // committed). Any backpressure markers `flush_drops` re-queued were
        // counted into `enqueued` on push and will settle on their own pass.
        // Advancing `settled` here wakes any parked `quiesce`.
        inner.settle_one();
    }
}

/// Block until either a new event arrives or the bus closes.
async fn next_event(inner: &Arc<Inner>) -> Option<RunEvent> {
    loop {
        {
            let mut q = inner.queue.lock().await;
            if let Some(e) = q.pop_front() {
                // We just freed a slot; wake any backpressured
                // producer that was waiting on a lifecycle-only queue.
                // The event is NOT settled until `handle_event` completes
                // in `consumer_loop` (which calls `settle_one`), so
                // `quiesce` keeps waiting until the recorder INSERT lands.
                inner.notify_producer.notify_waiters();
                return Some(e);
            }
            if inner.closed.load(Ordering::Acquire) {
                return None;
            }
        }
        inner.notify_consumer.notified().await;
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
async fn flush_drops(inner: &Arc<Inner>, just_handled: &RunEvent) {
    let markers: Vec<BackpressureDroppedEvent> = {
        let mut map = inner.drops.lock().await;
        if map.is_empty() {
            prune_span_map(&inner.span_to_run, just_handled).await;
            return;
        }
        let span_to_run = inner.span_to_run.lock().await;
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
        drop(span_to_run);
        prune_span_map(&inner.span_to_run, just_handled).await;
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
                note: "bus capacity exceeded; drops not attributable to a run".to_owned(),
            });
        }
        out
    };
    for marker in markers {
        publish_marker(inner, marker).await;
    }
}

async fn publish_marker(inner: &Arc<Inner>, marker: BackpressureDroppedEvent) {
    let event = RunEvent::BackpressureDropped(marker);
    {
        let mut q = inner.queue.lock().await;
        if q.len() < inner.capacity {
            q.push_back(event);
            inner.enqueued.fetch_add(1, Ordering::Release);
            inner.notify_consumer.notify_one();
            return;
        }
    }
    // Queue is still saturated. Re-park the count in the drops map so
    // the next `flush_drops` (triggered when the consumer drains one
    // of the events ahead in the queue) retries the marker. This is
    // strictly preferable to evicting another event here — a marker
    // recursively evicting another event would itself need to be
    // counted, leading to an unbounded chain.
    let RunEvent::BackpressureDropped(m) = event else {
        unreachable!()
    };
    let key = if m.run_id.is_empty() {
        DropKey::Unattributed
    } else {
        DropKey::Run(m.run_id.clone())
    };
    *inner.drops.lock().await.entry(key).or_insert(0) += m.dropped;
    warn!(
        target: "xvision_observability::bus",
        run_id = %m.run_id, dropped = m.dropped,
        "could not enqueue backpressure marker now; re-parked for next consumed event"
    );
}

async fn prune_span_map(span_to_run: &Mutex<HashMap<String, String>>, event: &RunEvent) {
    let run_id = match event {
        RunEvent::RunFinished(e) => Some(e.run_id.clone()),
        RunEvent::RunInterrupted(e) => Some(e.run_id.clone()),
        _ => None,
    };
    if let Some(run_id) = run_id {
        span_to_run.lock().await.retain(|_, r| r != &run_id);
    }
}

fn recorder_error(e: &RecorderError) {
    warn!(
        target: "xvision_observability::bus",
        error = %e,
        "recorder failed to handle event"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SpanStartedEvent;
    use crate::recorder::NoopRecorder;
    use crate::types::SpanKind;
    use chrono::Utc;

    fn span_event(i: usize) -> RunEvent {
        RunEvent::SpanStarted(SpanStartedEvent {
            span_id: format!("span-{i}"),
            run_id: "run-quiesce".to_string(),
            parent_span_id: None,
            kind: SpanKind::AgentDecision,
            name: format!("decision-{i}"),
            started_at: Utc::now(),
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        })
    }

    /// `quiesce` must block until EVERY published event has been handled by
    /// the subscriber — not merely yield once. This is the property the
    /// short-lived `xvn eval run` CLI depends on: publish the run's events,
    /// `quiesce`, exit, and the recorder has seen all of them. A single
    /// `yield_now` (the old behaviour) would let the consumer keep events
    /// in-flight and lose them on process exit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quiesce_drains_all_published_events_to_subscriber() {
        let recorder = Arc::new(NoopRecorder::new());
        let bus = RunEventBus::new(vec![recorder.clone()]);

        const N: usize = 64;
        for i in 0..N {
            bus.publish(span_event(i)).await;
        }

        // No sleeps, no polling: a single quiesce must guarantee the drain.
        bus.quiesce().await;

        let seen = recorder.snapshot().await;
        assert_eq!(
            seen.len(),
            N,
            "quiesce must drain every published event to the subscriber \
             before returning (saw {} of {N})",
            seen.len()
        );
    }

    /// On an already-idle bus `quiesce` returns immediately (nothing in
    /// flight), and remains correct when called repeatedly.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quiesce_is_noop_when_idle() {
        let recorder = Arc::new(NoopRecorder::new());
        let bus = RunEventBus::new(vec![recorder.clone()]);
        bus.quiesce().await;
        bus.publish(span_event(0)).await;
        bus.quiesce().await;
        bus.quiesce().await;
        assert_eq!(recorder.snapshot().await.len(), 1);
    }
}
