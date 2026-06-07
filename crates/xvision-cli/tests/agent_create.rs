//! Integration tests for `xvn agent create` tool grants. Spawns the built
//! binary against a tempdir-rooted `XVN_HOME` so the persisted agent shape goes
//! through the real engine API, store, and JSON emit path.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

/// Long-enough prompt to satisfy `validate_agent_for_save`'s content
/// gate (≥200 characters of actual content). Reused across tests.
const PROMPT: &str = "You are a regime filter for the trader agent. Inspect the supplied OHLCV context, recent volatility, and risk limits, and emit JSON {\"regime\": \"high_vol\" | \"low_vol\"} so the downstream trader knows when to dispatch. Stay grounded in the active market data.";

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

#[test]
fn agent_create_filter_persists_tools() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "test-filter-agent",
            "--tools",
            "indicator_panel",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            PROMPT,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(body["name"], "test-filter-agent");
    assert!(body["agent_id"].as_str().unwrap().starts_with('0'));
    let slot = &body["slots"][0];
    assert_eq!(slot["provider"], "anthropic");
    assert_eq!(slot["model"], "claude-haiku-4-5");
    let tools: Vec<String> = slot["allowed_tools"]
        .as_array()
        .expect("allowed_tools array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tools, vec!["indicator_panel".to_string()]);
}

#[test]
fn agent_create_trader_with_overrides_round_trips() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "test-trader-agent",
            "--tools",
            "ohlcv,submit_decision",
            "--provider",
            "openrouter",
            "--model",
            "anthropic/claude-3.5-sonnet",
            "--system-prompt",
            PROMPT,
            "--temperature",
            "0.2",
            "--max-tokens",
            "4096",
            "--tags",
            "smoke",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    let slot = &body["slots"][0];
    // `temperature` is intentionally not yet persisted to SQLite (see
    // AgentSlot::temperature doc-comment — a follow-up migration adds
    // the column). Until then, store round-trip returns None even when
    // the CLI wrote a value, so we assert only on the fields that
    // actually persist.
    assert_eq!(slot["max_tokens"], 4096);
    let tags: Vec<String> = body["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(tags.contains(&"smoke".to_string()));

    let tools: Vec<String> = slot["allowed_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tools, vec!["ohlcv".to_string(), "submit_decision".to_string()]);
}

#[test]
fn agent_create_at_prefix_reads_prompt_from_file() {
    let dir = tempdir().unwrap();
    let prompt_path = dir.path().join("filter_prompt.md");
    std::fs::write(&prompt_path, PROMPT).unwrap();

    let arg = format!("@{}", prompt_path.display());
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "test-prompt-from-file",
            "--tools",
            "indicator_panel",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            &arg,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        0,
        "expected exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout must be JSON");
    assert_eq!(body["slots"][0]["system_prompt"], PROMPT);
}

// ── Failure modes — exit code 2 (Usage) ───────────────────────────────────

#[test]
fn agent_create_deprecated_capability_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "deprecated-capability",
            "--capability",
            "filter",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            PROMPT,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 on deprecated --capability; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("use --tools instead"));
}

#[test]
fn agent_create_empty_prompt_after_file_read_returns_usage() {
    let dir = tempdir().unwrap();
    let prompt_path = dir.path().join("empty.md");
    std::fs::write(&prompt_path, "   \n\n   ").unwrap();
    let arg = format!("@{}", prompt_path.display());

    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "empty-prompt",
            "--tools",
            "indicator_panel",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            &arg,
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 when prompt file is empty; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn agent_create_unreadable_at_path_returns_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &[
            "agent",
            "create",
            "--name",
            "missing-prompt-file",
            "--tools",
            "indicator_panel",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            "@/does/not/exist/prompt.md",
        ],
        dir.path(),
    );
    assert_eq!(
        code(&out),
        2,
        "expected exit 2 when --system-prompt @path is unreadable; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
