//! §2-D review nit #1 — `xvn trajectory` must read recordings from the SAME
//! migrated DB the record path writes to.
//!
//! Before the fix, the `xvn trajectory inspect|validate|purge|reindex`
//! subcommands opened a raw, un-migrated `$XVN_HOME/data/store.db`. But the
//! eval record path persists recordings into the `ApiContext` DB
//! (`$XVN_HOME/xvn.db`, which owns migration 040) with blobs under
//! `$XVN_HOME/agent_runs/blobs` (see `api::eval` recording wiring +
//! `cline_recording::open_store`). So the CLI could never read a real
//! recording.
//!
//! This test reproduces the production record-path DB target exactly:
//!   1. `ApiContext::open($XVN_HOME)` migrates `$XVN_HOME/xvn.db` (incl. 040).
//!   2. A recording is minted + a frame appended via the production
//!      `cline_recording` helpers against `ctx.db` + `$XVN_HOME/agent_runs/blobs`
//!      — the EXACT pool + blob root `api::eval::spawn_cline_ctx` uses.
//!   3. The CLI's `trajectory::open_store` (with `XVN_HOME` set, no `--db`
//!      override) opens the store and reads the recording back — proving the
//!      repointed CLI targets the same DB the record path wrote to.

use std::sync::Arc;

use tempfile::TempDir;

use xvision_engine::agent::cline_recording;
use xvision_engine::api::{Actor, ApiContext};
use xvision_observability::trajectory::frame::TrajectoryFrame;

const SLOT_ROLE: &str = "trader";

#[tokio::test]
async fn cli_open_store_reads_recording_written_to_migrated_xvn_db() {
    let home = TempDir::new().unwrap();
    std::env::set_var("XVN_HOME", home.path());

    // (1) Open the ApiContext — migrates `$XVN_HOME/xvn.db` including
    // migration 040 (the trajectory tables). This is the exact DB the record
    // path writes to (`ctx.db`).
    let ctx = ApiContext::open(home.path(), Actor::Cli { user: "test".into() })
        .await
        .expect("open ApiContext (migrates xvn.db + trajectory tables)");

    // (2) Mint a recording + append a frame through the production helpers,
    // against the SAME pool + blob root the eval record path uses:
    //   pool      = ctx.db
    //   blob_root = $XVN_HOME/agent_runs/blobs
    let blob_root = ctx.xvn_home.join("agent_runs").join("blobs");
    let store = Arc::new(
        cline_recording::open_store(ctx.db.clone(), blob_root)
            .await
            .expect("open trajectory store over the migrated xvn.db"),
    );
    let key = cline_recording::build_key("cli-read-run-1", SLOT_ROLE, "anthropic", "claude-sonnet-4-6");
    let rid = cline_recording::begin(&store, &key)
        .await
        .expect("begin recording");

    // Append one frame so frame_counts/inspect have something to report.
    store
        .append_frame(
            &rid,
            SLOT_ROLE,
            0,
            0,
            &TrajectoryFrame::ToolCallDelta {
                ts_ms: 1,
                tool_call_id: Some("c1".into()),
                tool_name: Some("submit_decision".into()),
                input: Some(serde_json::json!({"action": "long_open"})),
            },
        )
        .await
        .expect("append frame");

    // Drop the writer store so the WAL is visible to a fresh reader pool.
    drop(store);

    // (3) The CLI's open_store (no --db override) must resolve `$XVN_HOME` →
    // `$XVN_HOME/xvn.db` via ApiContext and read the recording back. Before the
    // fix it pointed at `$XVN_HOME/data/store.db` and would 404.
    let cli_store = xvision_cli::commands::trajectory::open_store(None, None)
        .await
        .expect("CLI trajectory open_store must open the migrated xvn.db");

    let info = cli_store
        .get_recording(rid.as_str())
        .await
        .expect("CLI store must read the recording the record path wrote");
    assert_eq!(info.recording_id, rid.as_str());
    assert_eq!(info.slot_role, SLOT_ROLE);

    let counts = cli_store.frame_counts(rid.as_str()).await.expect("frame_counts");
    let total: i64 = counts.iter().map(|c| c.count).sum();
    assert_eq!(total, 1, "the appended frame is visible through the CLI store");

    std::env::remove_var("XVN_HOME");
}
