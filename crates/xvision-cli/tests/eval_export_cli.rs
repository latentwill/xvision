//! End-to-end check for `xvn eval export <run_id>`. Seeds a Run via the
//! engine's `RunStore`, invokes the CLI, and confirms the stdout JSON
//! carries the full q15 §3 envelope.

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

async fn seed_run(home: &std::path::Path) -> String {
    // ApiContext::open applies migrations on first touch, so the seeded
    // run lands in a fully-initialized SQLite file the CLI process will
    // open via the same path.
    let ctx = ApiContext::open(
        home,
        Actor::Cli {
            user: "eval-export-cli-test".into(),
        },
    )
    .await
    .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());
    // `crypto-bull-q1-2025` is one of the canonical scenarios seeded by
    // the engine on first migration; using it lets the run row pass the
    // FK check on `eval_runs.scenario_id`.
    let run = Run::new_queued("agent-X".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.expect("seed run");
    // Export is terminal-only — drive the seeded run to Completed so
    // the CLI returns the snapshot instead of an exit-3 validation
    // error.
    store
        .update_status(&id, RunStatus::Completed, None)
        .await
        .expect("transition to terminal");
    id
}

#[test]
fn eval_export_stdout_carries_full_envelope() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = rt.block_on(async { seed_run(dir.path()).await });

    let out = xvn(&["eval", "export", &run_id], dir.path());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    assert_eq!(body["schema_version"], "1");
    assert_eq!(body["run"]["id"], run_id);

    // Every spec top-level key is present so consumers don't need to
    // optional-chain through the shape.
    for key in [
        "schema_version",
        "run",
        "scenario",
        "strategy",
        "agents",
        "metrics",
        "decisions",
        "equity_samples",
        "events",
        "errors",
        "reviews",
        "provider_diagnostics",
    ] {
        assert!(body.get(key).is_some(), "missing top-level key `{key}` in {body}");
    }
}

#[test]
fn eval_export_output_flag_writes_byte_identical_file() {
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = rt.block_on(async { seed_run(dir.path()).await });

    let out_path = dir.path().join("export.json");
    let cli_out = xvn(
        &[
            "eval",
            "export",
            &run_id,
            "--output",
            out_path.to_str().expect("utf8 path"),
        ],
        dir.path(),
    );
    assert!(
        cli_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cli_out.stderr)
    );

    let file_bytes = std::fs::read(&out_path).expect("file written");
    let body: serde_json::Value = serde_json::from_slice(&file_bytes).expect("file must be valid JSON");
    assert_eq!(body["schema_version"], "1");
    assert_eq!(body["run"]["id"], run_id);

    // Stdout in --output mode is empty; the byte total of the file
    // matches the report on stderr ("eval export → … (N bytes)").
    assert!(
        cli_out.stdout.is_empty(),
        "stdout: {}",
        String::from_utf8_lossy(&cli_out.stdout)
    );
    let stderr = String::from_utf8_lossy(&cli_out.stderr);
    assert!(stderr.contains("eval export"), "stderr: {stderr}");
    let bytes_marker = format!("{} bytes", file_bytes.len());
    assert!(stderr.contains(&bytes_marker), "stderr: {stderr}");
}

#[test]
fn eval_export_in_flight_run_returns_2_usage() {
    // A Queued run is not terminal — export must reject upstream
    // (ApiError::Validation -> XvnExit::Usage = 2) instead of writing
    // a moving snapshot.
    let dir = tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let run_id = rt.block_on(async {
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "eval-export-cli-test".into(),
            },
        )
        .await
        .expect("open ApiContext");
        let store = RunStore::new(ctx.db.clone());
        // Leave the run in Queued — no update_status transition.
        let run = Run::new_queued("agent-X".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        let id = run.id.clone();
        store.create(&run).await.expect("seed run");
        id
    });

    let out = xvn(&["eval", "export", &run_id], dir.path());
    let code = out.status.code().expect("child terminated by signal");
    assert_eq!(
        code,
        2,
        "expected XvnExit::Usage on terminal-only violation, stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}
