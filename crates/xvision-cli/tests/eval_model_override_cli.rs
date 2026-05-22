//! CLI-level coverage for the per-launch `xvn eval run --provider --model`
//! override (Wave B #5, contract `cli-eval-model-override`).
//!
//! The full eval-launch flow needs broker / dispatch / scenario fixtures
//! that the engine-level test already covers (`eval_model_override.rs`).
//! Here we assert the CLI's responsibility:
//!
//! 1. **Both flags or neither.** `--provider X` without `--model Y` (and
//!    vice versa) exits 2 (usage) before the engine is touched, with a
//!    message naming the missing flag.
//! 2. **Override receipt round-trips into `eval results --json`.** Seeds
//!    a completed run with a `supervisor_notes` row carrying the
//!    `provider_override` payload, runs `xvn eval results <id> --json`,
//!    and asserts the JSON body's `provider_override.provider` /
//!    `.model` match what was persisted. The override is per-run; the
//!    strategy's bound provider/model on disk is unchanged.
//! 3. **`--help` documents both flags.** The flag names are part of the
//!    operator-visible contract; a rename here would break scripts.

use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::run::{MetricsSummary, Run, RunMode};
use xvision_engine::eval::store::RunStore;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

/// Tolerate any tracing/ANSI prefix on stdout (`cli-json-stdout-contract`
/// already fixed this for canonical verbs, but we mirror the lenient
/// parser used by `eval_results_report.rs` so this test is robust under
/// concurrent stdout-discipline tightening).
fn parse_json_lenient(stdout: &[u8]) -> Value {
    let s = std::str::from_utf8(stdout).expect("stdout is utf-8");
    let start = s
        .find("{\n")
        .or_else(|| s.find("{ "))
        .or_else(|| s.find('{'))
        .expect("no JSON object in stdout");
    serde_json::from_str(&s[start..]).expect("parse json")
}

#[test]
fn eval_run_provider_without_model_exits_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "eval", "run", "--strategy", "any", "--scenario", "any", "--provider", "anthropic",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 (usage) for partial override, stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--model") || stderr.contains("model"),
        "usage error must name the missing --model flag: stderr={stderr}",
    );
}

#[test]
fn eval_run_model_without_provider_exits_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "eval",
            "run",
            "--strategy",
            "any",
            "--scenario",
            "any",
            "--model",
            "claude-sonnet-4.6",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 (usage) for partial override, stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--provider") || stderr.contains("provider"),
        "usage error must name the missing --provider flag: stderr={stderr}",
    );
}

async fn seed_completed_run_with_override(
    ctx: &ApiContext,
    override_provider: &str,
    override_model: &str,
) -> String {
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(
        "agent-override-cli-test".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let run_id = run.id.clone();
    store.create(&run).await.expect("create run");
    store
        .finalize(
            &run_id,
            &MetricsSummary {
                total_return_pct: 1.5,
                sharpe: 0.6,
                max_drawdown_pct: 0.5,
                win_rate: 0.5,
                n_trades: 0,
                n_decisions: 0,
                baselines: None,
                ..Default::default()
            },
        )
        .await
        .expect("finalize run");
    // Mirror the production write path: a `supervisor_notes` row with
    // role=`provider_override` carrying the JSON `{provider, model}`.
    let payload = serde_json::json!({
        "provider": override_provider,
        "model": override_model,
    });
    store
        .record_supervisor_note(&run_id, "provider_override", "info", &payload.to_string())
        .await
        .expect("record provider_override supervisor note");
    run_id
}

#[tokio::test]
async fn eval_results_json_surfaces_provider_override_when_present() {
    let home = tempdir().unwrap();
    let ctx = ApiContext::open(home.path(), Actor::Cli { user: "override-cli-test".into() })
        .await
        .expect("open ApiContext");
    let run_id = seed_completed_run_with_override(&ctx, "openrouter", "deepseek/deepseek-v4-flash").await;
    drop(ctx);

    let out = xvn(&["eval", "results", &run_id, "--json"], home.path());
    assert!(
        out.status.success(),
        "xvn eval results exit={:?} stderr={} stdout={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );

    let body: Value = parse_json_lenient(&out.stdout);
    let po = body
        .get("provider_override")
        .expect("provider_override field on results --json");
    assert_eq!(
        po.get("provider").and_then(Value::as_str),
        Some("openrouter"),
        "override provider must match the persisted receipt: body={body}",
    );
    assert_eq!(
        po.get("model").and_then(Value::as_str),
        Some("deepseek/deepseek-v4-flash"),
        "override model must match the persisted receipt: body={body}",
    );
}

#[tokio::test]
async fn eval_results_json_omits_provider_override_when_absent() {
    let home = tempdir().unwrap();
    let ctx = ApiContext::open(home.path(), Actor::Cli { user: "no-override-cli".into() })
        .await
        .expect("open ApiContext");
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued(
        "agent-no-override".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    let run_id = run.id.clone();
    store.create(&run).await.expect("create run");
    store
        .finalize(
            &run_id,
            &MetricsSummary {
                total_return_pct: 0.0,
                sharpe: 0.0,
                max_drawdown_pct: 0.0,
                win_rate: 0.0,
                n_trades: 0,
                n_decisions: 0,
                baselines: None,
                ..Default::default()
            },
        )
        .await
        .expect("finalize run");
    drop(ctx);

    let out = xvn(&["eval", "results", &run_id, "--json"], home.path());
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let body: Value = parse_json_lenient(&out.stdout);
    assert!(
        body.get("provider_override").is_none(),
        "provider_override must be absent when no override was applied: body={body}",
    );
}

#[test]
fn eval_run_help_documents_provider_and_model_flags() {
    // Operator contract: --provider / --model appear on `xvn eval run --help`.
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "run", "--help"], dir.path());
    assert_eq!(code(&out), 0, "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--provider"),
        "`xvn eval run --help` must document --provider: stdout={stdout}",
    );
    assert!(
        stdout.contains("--model"),
        "`xvn eval run --help` must document --model: stdout={stdout}",
    );
}
