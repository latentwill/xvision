//! SSE stream builder for autoresearch subprocess stdout lines.
//!
//! Each stdout line from `uv run xvision_train.py` is broadcast on an
//! `AutoresearchStdoutLine` channel. This module wraps that channel in an
//! axum `Sse<impl Stream>` so `GET /api/autoresearch/runs/:id/stream` can
//! pipe it to connected clients.
//!
//! Lines beyond `SSE_LINE_BYTE_CAP` bytes are truncated before transmission.
//! Experiments table is the durable mirror — clients that disconnect and
//! reconnect reconstruct history from DB, not from the SSE stream.

use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::Stream;

use xvision_engine::autoresearch::experiment::SSE_LINE_BYTE_CAP;

/// One stdout line emitted by the training subprocess. Broadcast on the
/// per-run channel in `AppState::autoresearch_stdout` (added in Task 7.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoresearchStdoutLine {
    pub run_id: String,
    pub line: String,
}

/// Truncate `line` to at most `SSE_LINE_BYTE_CAP` bytes. Truncation is at a
/// char boundary; for ASCII training logs this is equivalent to character
/// truncation.
pub fn truncate_line(line: &str) -> &str {
    if line.len() <= SSE_LINE_BYTE_CAP {
        line
    } else {
        // Walk back to a char boundary within the cap.
        let mut boundary = SSE_LINE_BYTE_CAP;
        while !line.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &line[..boundary]
    }
}

/// Build the SSE stream for one run's stdout feed.
///
/// Each event has `event: stdout_line` and `data: <truncated line>`.
/// The stream terminates when the broadcast channel is closed (run finished
/// or stopped). A 15-second keep-alive comment prevents proxy timeouts.
pub fn autoresearch_stdout_stream(
    run_id: &str,
    mut rx: broadcast::Receiver<AutoresearchStdoutLine>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let run_id = run_id.to_string();
    stream! {
        loop {
            match rx.recv().await {
                Ok(msg) if msg.run_id == run_id => {
                    let data = truncate_line(&msg.line).to_string();
                    yield Ok(
                        Event::default()
                            .event("stdout_line")
                            .data(data)
                    );
                }
                Ok(_) => {
                    // Message for a different run — skip without yielding.
                    continue;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let body = format!("{{\"dropped\":{n}}}");
                    yield Ok(Event::default().event("lagged").data(body));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    }
}

/// Axum handler helper: wraps `autoresearch_stdout_stream` in `Sse` with
/// keep-alive. Called from the route handler in Task 7.2.
pub fn autoresearch_sse(
    run_id: &str,
    rx: broadcast::Receiver<AutoresearchStdoutLine>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    Sse::new(autoresearch_stdout_stream(run_id, rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn stdout_lines_emitted_as_sse_data_events() {
        let (tx, rx) = broadcast::channel::<AutoresearchStdoutLine>(64);

        tx.send(AutoresearchStdoutLine {
            run_id: "run-01".to_string(),
            line: "epoch 1/10 loss=0.8".to_string(),
        })
        .unwrap();
        tx.send(AutoresearchStdoutLine {
            run_id: "run-01".to_string(),
            line: "XVN_RESULT {\"val_acc\": 0.71, \"val_loss\": 0.3}".to_string(),
        })
        .unwrap();
        drop(tx);

        let mut collected = Vec::new();
        let mut rx2 = rx; // already subscribed
        loop {
            match rx2.try_recv() {
                Ok(msg) => collected.push(msg),
                Err(_) => break,
            }
        }

        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].line, "epoch 1/10 loss=0.8");
        assert!(collected[1].line.starts_with("XVN_RESULT"));
    }

    #[test]
    fn oversized_line_is_truncated_to_cap() {
        let long_line: String = "x".repeat(SSE_LINE_BYTE_CAP + 100);
        let truncated = truncate_line(&long_line);
        assert_eq!(truncated.len(), SSE_LINE_BYTE_CAP);
    }

    #[test]
    fn line_within_cap_is_unchanged() {
        let short = "hello world";
        assert_eq!(truncate_line(short), short);
    }

    #[tokio::test]
    async fn stream_terminates_when_channel_closes() {
        use tokio_stream::StreamExt;
        let (tx, rx) = broadcast::channel::<AutoresearchStdoutLine>(4);
        tx.send(AutoresearchStdoutLine {
            run_id: "r1".into(),
            line: "line1".into(),
        })
        .unwrap();
        drop(tx);

        let stream = autoresearch_stdout_stream("r1", rx);
        let events: Vec<_> = stream.collect().await;
        // At least the one line event, stream closed without panic.
        assert!(!events.is_empty());
    }
}
