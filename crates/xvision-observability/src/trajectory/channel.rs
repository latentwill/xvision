//! Lossless backpressured frame channel (item 4).
//!
//! ## Design: lossless vs lossy
//!
//! The observability `RunEventBus` is **lossy by design**: it drops the
//! oldest non-lifecycle event when full, because backpressure on the event
//! bus would stall the agent executor.  Dropping a delta is acceptable
//! because observability data is sampled / approximate.
//!
//! Trajectory frames are **non-droppable**: a dropped frame breaks replay
//! determinism.  Therefore `FrameChannel` uses a `tokio::sync::mpsc`
//! bounded channel whose `send().await` applies **true backpressure** — the
//! producer awaits until a slot is available rather than dropping.
//!
//! ## Drop-of-last-resort
//!
//! The only way a frame can be lost is if the consumer task has exited
//! fatally (e.g. the storage backend panicked and the receiver was dropped).
//! In that case `send()` returns `Err`.  The caller **must** call
//! `FrameChannel::mark_corrupt(reason)` in that path so the recording is
//! not silently usable for replay.
//!
//! ## Capacity
//!
//! `FrameChannel::new(cap)` accepts a capacity.  The default is 1024.
//! Under normal conditions the channel should be nearly empty; capacity
//! only matters for bursty multi-step slots where the storage I/O is
//! momentarily slower than the model streaming.

use crate::trajectory::frame::TrajectoryFrame;
use tokio::sync::mpsc;

/// Default frame-channel capacity.  One recording spans a slot's full
/// multi-step trajectory; 1024 frames is ~50+ steps of realistic depth
/// with 20 frames/step.
pub const DEFAULT_FRAME_CHANNEL_CAPACITY: usize = 1024;

/// Status set when the channel is closed abnormally (consumer died).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelStatus {
    Open,
    /// Consumer accepted all frames and signalled done normally.
    Closed,
    /// Consumer died before draining all frames.  The recording MUST be
    /// marked `corrupt` with `reason`.
    Corrupt { reason: String },
}

/// Sender half — held by the producer (the agent harness / sidecar client).
pub struct FrameSender {
    tx: mpsc::Sender<TrajectoryFrame>,
    capacity: usize,
}

impl FrameSender {
    /// Send a frame, **awaiting** under backpressure (never drops).
    ///
    /// Returns `Err(frame)` only if the receiver has been dropped (consumer
    /// died).  The caller must handle this by marking the recording corrupt.
    pub async fn send(&self, frame: TrajectoryFrame) -> Result<(), TrajectoryFrame> {
        self.tx.send(frame).await.map_err(|e| e.0)
    }

    /// Capacity the channel was constructed with.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Receiver half — held by the consumer (the trajectory store writer task).
pub struct FrameReceiver {
    rx: mpsc::Receiver<TrajectoryFrame>,
    capacity: usize,
}

impl FrameReceiver {
    /// Receive the next frame, or `None` if the sender was dropped.
    pub async fn recv(&mut self) -> Option<TrajectoryFrame> {
        self.rx.recv().await
    }

    /// Capacity the channel was constructed with.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// A `FrameChannel` bundles the sender + receiver + status tracker.
///
/// In production usage the sender and receiver are split and each is moved
/// to its respective task.  The status is tracked externally (on the
/// recording row) by the caller.
pub struct FrameChannel {
    capacity: usize,
    sender: FrameSender,
    receiver: FrameReceiver,
}

impl FrameChannel {
    /// Create a new bounded channel with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        Self {
            capacity,
            sender: FrameSender { tx, capacity },
            receiver: FrameReceiver { rx, capacity },
        }
    }

    /// Create with the default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_FRAME_CHANNEL_CAPACITY)
    }

    /// Split into (sender, receiver).  Consumes `self`.
    pub fn split(self) -> (FrameSender, FrameReceiver) {
        (self.sender, self.receiver)
    }

    /// Channel capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn bounds_are_explicit() {
        let ch = FrameChannel::new(16);
        assert_eq!(ch.capacity(), 16);
        let (tx, rx) = ch.split();
        assert_eq!(tx.capacity(), 16);
        assert_eq!(rx.capacity(), 16);
    }

    #[tokio::test]
    async fn frames_are_never_dropped_under_pressure() {
        // Fill the channel past capacity from a producer; assert the
        // producer blocks/awaits and zero frames are lost once it drains.
        let cap = 4;
        let (tx, mut rx) = FrameChannel::new(cap).split();
        let tx = Arc::new(tx);

        let n_frames = 20usize;
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent.clone();

        // Producer — will block once cap is reached until consumer drains.
        let producer = tokio::spawn(async move {
            for i in 0..n_frames {
                let f = TrajectoryFrame::TextDelta {
                    ts_ms: i as u64,
                    text: format!("frame-{i}"),
                };
                sent_clone.lock().await.push(f.clone());
                tx.send(f).await.expect("receiver alive");
            }
        });

        // Consumer — drain all frames.
        let mut received = Vec::new();
        while received.len() < n_frames {
            let f = rx.recv().await.expect("sender alive");
            received.push(f);
        }

        producer.await.expect("producer finished");

        // All frames received in order, no drops.
        assert_eq!(received.len(), n_frames);
        for (i, f) in received.iter().enumerate() {
            if let TrajectoryFrame::TextDelta { ts_ms, text } = f {
                assert_eq!(*ts_ms, i as u64);
                assert_eq!(text, &format!("frame-{i}"));
            } else {
                panic!("unexpected variant");
            }
        }
    }

    #[tokio::test]
    async fn dropped_frame_invalidates_recording() {
        // Force a drop by dropping the receiver while the sender still holds frames.
        // The send() call should return Err indicating the receiver is gone.
        // The caller must mark the recording corrupt.
        let cap = 2;
        let (tx, rx) = FrameChannel::new(cap).split();

        // Fill to capacity.
        let f1 = TrajectoryFrame::TextDelta { ts_ms: 1, text: "a".into() };
        let f2 = TrajectoryFrame::TextDelta { ts_ms: 2, text: "b".into() };
        tx.send(f1).await.unwrap();
        tx.send(f2).await.unwrap();

        // Drop the receiver — simulates a fatal consumer crash.
        drop(rx);

        // The next send must fail (receiver gone), signalling the caller
        // to mark the recording corrupt.
        let f3 = TrajectoryFrame::TextDelta { ts_ms: 3, text: "c".into() };
        let result = tx.send(f3).await;
        assert!(
            result.is_err(),
            "send should fail when receiver is dropped"
        );
        // Verify the returned frame is the one we tried to send.
        let returned = result.unwrap_err();
        if let TrajectoryFrame::TextDelta { ts_ms, .. } = returned {
            assert_eq!(ts_ms, 3);
        } else {
            panic!("wrong frame returned");
        }
        // In production code the caller would now call
        // `store.mark_corrupt(recording_id, "consumer died").await`.
        // The channel itself does not reach into the store; it only
        // signals via the Err return so the caller can do so.
    }

    #[tokio::test]
    async fn receiver_returns_none_when_sender_dropped() {
        let (tx, mut rx) = FrameChannel::new(8).split();
        drop(tx);
        assert!(rx.recv().await.is_none());
    }
}
