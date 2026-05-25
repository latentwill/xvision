//! Checkpoint + restore for the chat rail (Phase 2.5).
//!
//! A [`Checkpointer`] takes an immutable, content-addressed snapshot of a chat
//! session's mutable authoring artifacts — the Strategy JSON on disk, the
//! agent-slot rows in the DB, the session's tool policy, and the focus file —
//! and can later [`restore`](Checkpointer::restore) them verbatim. The snapshot
//! is taken *before* a mutating tool runs so an operator can rewind a bad edit.
//!
//! ## Content-addressing
//!
//! Each captured artifact's raw payload bytes are written to a [`BlobStore`]
//! (mirroring `xvision-observability::blobs`). A single `captured_json`
//! manifest records the per-artifact `{ kind, blob_hash, meta }` triples plus
//! the metadata needed to write each one back (the strategy id, the agent id,
//! the focus path). The checkpoint row stores that manifest plus a
//! `content_hash` = sha256 of the canonical manifest JSON.
//!
//! ## Verbatim, non-destructive restore
//!
//! Restore reads the blobs and rewinds each artifact by writing the *exact*
//! captured bytes back — so a byte-compare of a restored Strategy file against
//! the original at snapshot time is identical. Restore validates the entire
//! plan up front (every referenced blob must exist) and only then writes; a
//! missing blob or an unknown checkpoint id surfaces a typed
//! [`CheckpointError`] and touches nothing. The blob store and the strategy
//! filesystem are append-only here — restore never deletes.
//!
//! ## Scope
//!
//! This module is a standalone engine capability. It does NOT decide *when* to
//! snapshot — the "snapshot before a mutating tool" hook is wired by the rail
//! integration (the conductor), not here. The dashboard route in
//! `xvision-dashboard::routes::checkpoints` exposes list + restore over HTTP.

mod store;

pub use store::{
    CapturedArtifact, CapturedManifest, Checkpoint, CheckpointError, CheckpointKind, Checkpointer,
    RestoreOutcome, SnapshotRequest,
};
