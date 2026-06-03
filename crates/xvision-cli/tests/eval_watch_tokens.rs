//! Integration test for `xvn eval watch --json` live token/cost shape.
//!
//! Seeds a Run via the engine's `RunStore`, invokes `xvn eval watch <id>
//! --once --json`, and asserts the top-level JSON is the `{ run, tokens }`
//! block (not the bare `Run`) — the contract change that lets the CLI surface
//! the same live token/cost the dashboard shows. A freshly-seeded run has no
//! `model_calls`, so the `tokens` block carries null totals with
//! `model_call_count = 0`; the shape (keys present) is what we pin here.

use std::process::Command;

use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{Run, RunMode, RunStatus};
use xvision_engine::eval::store::RunStore;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

async fn seed_running_run(home: &std::path::Path) -> String {
    let ctx = ApiContext::open(
        home,
        Actor::Cli {
            user: "eval-watch-tokens-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(
        "agent-tok".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let id = run.id.clone();
    store.create(&run).await.expect("seed run");
    store
        .update_status(&id, RunStatus::Running, None)
        .await
        .expect("transition run to running");
    id
}

#[test]
fn watch_json_emits_run_and_tokens_block() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let run_id = rt.block_on(async { seed_running_run(dir.path()).await });

    let out = xvn(&["eval", "watch", &run_id, "--once", "--json"], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");

    // Top-level is the sibling block, not the bare Run.
    let run = body.get("run").expect("top-level `run` key");
    assert_eq!(
        run["id"].as_str(),
        Some(run_id.as_str()),
        "run.id should round-trip"
    );

    let tokens = body.get("tokens").expect("top-level `tokens` key");
    assert!(tokens.is_object(), "tokens must be an object: {tokens}");
    // Real `RunTokenTotals` field names (struct serialized directly).
    assert!(
        tokens.get("input_tokens").is_some(),
        "tokens.input_tokens key present"
    );
    assert!(
        tokens.get("output_tokens").is_some(),
        "tokens.output_tokens key present"
    );
    assert!(
        tokens.get("cost_estimate_complete").is_some(),
        "tokens.cost_estimate_complete key present"
    );
    // No model_calls landed for a freshly-seeded run → "no signal" totals.
    assert_eq!(
        tokens["model_call_count"].as_u64(),
        Some(0),
        "freshly-seeded run has zero model_calls"
    );
    assert!(
        tokens["input_tokens"].is_null(),
        "input_tokens null with no signal"
    );
}
