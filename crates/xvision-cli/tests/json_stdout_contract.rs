//! Golden test for the `--json` stdout-discipline contract
//! (`team/contracts/cli-json-stdout-contract.md`).
//!
//! When a CLI verb is invoked with `--json`, **stdout must contain
//! exactly one parseable JSON value and nothing else.** Progress text,
//! banners, completion summaries, deprecation notices — all of that
//! routes to stderr.
//!
//! ## How the test works
//!
//! For each verb in the audit list, the test invokes `xvn ... --json`
//! against a tempdir-scoped `XVN_HOME`. The invocation is expected to
//! fail at the engine layer (no strategy / no scenario / no run id in
//! the empty home), but **the stdout it produces before failing must
//! still be a single parseable JSON value or empty**. Stdout is not
//! parsed against a schema — only that `serde_json::from_slice` on the
//! captured bytes succeeds (when stdout is non-empty), and that the
//! captured bytes contain none of the banner markers
//! (`[INFO]`, `→`, `====`, `"completed."`).
//!
//! Stderr is *unconstrained*: every verb is free to print as many
//! progress lines as it likes there.
//!
//! ## Empty stdout is acceptable
//!
//! Several verbs error out (validation, not-found, usage) before
//! reaching the `--json` branch — clap rejects an arg or the engine
//! returns NotFound. In those cases the verb prints nothing to stdout
//! (the error goes to stderr) and we treat that as conforming to the
//! contract. The contract is "stdout is structured iff --json is set
//! AND the verb produced output," not "stdout is non-empty."
//!
//! ## Verb-by-verb commentary lives in `Case::comment`
//!
//! Each entry in `CASES` carries a `comment` field explaining what
//! shape, if any, was expected. A failure prints the case description
//! so the operator can match the test row to the contract audit list.

use std::process::Command;

use tempfile::tempdir;

/// One row in the audit list. `description` is the short label that
/// shows in the assertion failure; `args` is the verb + flags handed to
/// `xvn` (always followed by `--json` if `pass_json` is true).
struct Case {
    description: &'static str,
    args: &'static [&'static str],
    /// When true, the test appends `--json` to `args`. Always true for
    /// the audit list — we keep the field explicit so a future
    /// follow-up that wants to exercise the no-json default can flip it.
    pass_json: bool,
    /// One-shot polling form (e.g. `xvn eval watch --once`) — used so
    /// the watch verb doesn't loop forever during the test.
    extra_args: &'static [&'static str],
}

/// Banner markers that must never appear on stdout when `--json` is
/// set. The set is intentionally narrow — broader markers (e.g. plain
/// ASCII spaces, common JSON-control chars) would false-positive.
const FORBIDDEN_MARKERS: &[&str] = &[
    "[INFO]",
    "====",
    "Starting eval run",
    "Run completed.",
    "completed.",
    " → ",
    "Plan summary",
    "experiment-run plan",
];

const CASES: &[Case] = &[
    // ── eval ──────────────────────────────────────────────────────────────
    Case {
        description: "xvn eval run --json (no-such-strategy)",
        args: &[
            "eval",
            "run",
            "--strategy",
            "no-such-strategy",
            "--scenario",
            "no-such-scenario",
        ],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval list --json (empty home)",
        args: &["eval", "list"],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval show --json (unknown run)",
        args: &["eval", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval results --json (unknown run)",
        args: &["eval", "results", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval watch --json --once (unknown run)",
        args: &["eval", "watch", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: true,
        extra_args: &["--once"],
    },
    Case {
        description: "xvn eval batch run --json (unknown scenario)",
        args: &[
            "eval",
            "batch",
            "run",
            "--strategy",
            "no-such-strategy",
            "--scenarios",
            "no-such-scenario",
            "--wait",
        ],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval batch status --json (unknown batch)",
        args: &["eval", "batch", "status", "batch_01ZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval compare --json (two unknown runs)",
        args: &[
            "eval",
            "compare",
            "01ZZZZZZZZZZZZZZZZZZZZZZZZ",
            "02ZZZZZZZZZZZZZZZZZZZZZZZZ",
        ],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval cancel --json (unknown run id)",
        args: &["eval", "cancel", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn eval validate --json (unknown strategy)",
        args: &[
            "eval",
            "validate",
            "--strategy",
            "no-such-strategy",
            "--scenario",
            "no-such-scenario",
        ],
        pass_json: true,
        extra_args: &[],
    },
    // ── experiment ────────────────────────────────────────────────────────
    Case {
        description: "xvn experiment ls --json (empty)",
        args: &["experiment", "ls"],
        pass_json: true,
        extra_args: &[],
    },
    Case {
        description: "xvn experiment show --json (unknown id)",
        args: &["experiment", "show", "exp_01ZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: true,
        extra_args: &[],
    },
    // experiment run is a side-effecting verb; with --json on the no-yes
    // path it returns Usage (clap-level), so stdout stays empty. Still
    // worth a row to confirm no banner leaks to stdout before the error.
    Case {
        description: "xvn experiment run --json (dry-run, no --yes)",
        args: &[
            "experiment",
            "run",
            "--name",
            "test-exp",
            "--strategy",
            "no-such-strategy",
            "--scenarios",
            "no-such-scenario",
        ],
        pass_json: true,
        extra_args: &[],
    },
    // ── strategy ──────────────────────────────────────────────────────────
    Case {
        description: "xvn strategy ls --json (empty)",
        args: &["strategy", "ls"],
        pass_json: true,
        extra_args: &[],
    },
    // strategy show uses --format json (default), not --json. Already
    // routes through json::emit_object which is stdout-only. Audit row
    // exists to confirm no regression.
    Case {
        description: "xvn strategy show (unknown id, --format json)",
        args: &["strategy", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        pass_json: false,
        extra_args: &["--format", "json"],
    },
    // ── provider ──────────────────────────────────────────────────────────
    // `xvn provider list --json` is on the audit list but the `--json`
    // flag is added by the parallel `provider-resolution-parity`
    // contract. Until that lands, invoking with `--json` exits via clap
    // (stderr-only, empty stdout). The row stays so the test enforces
    // the channel-discipline when the flag arrives.
    Case {
        description: "xvn provider list --json (flag may be added by parity track)",
        args: &["provider", "list"],
        pass_json: true,
        extra_args: &[],
    },
    // ── doctor ─────────────────────────────────────────────────────────────
    Case {
        description: "xvn doctor --json (empty home)",
        args: &["doctor"],
        pass_json: true,
        extra_args: &[],
    },
    // ── agent ──────────────────────────────────────────────────────────────
    // `xvn agent ls --format json` uses --format, not --json; this row
    // exercises the ObjectFormat-style JSON path (distinct from --json flag).
    // An empty workspace returns an empty JSON array on stdout.
    Case {
        description: "xvn agent ls --format json (empty home)",
        args: &["agent", "ls", "--format", "json"],
        pass_json: false,
        extra_args: &[],
    },
    // `xvn agent create --dry-run --format json-compact` must emit a
    // single-line JSON object on stdout (no trailing text, no newline
    // beyond the one that terminates the println!). No XVN_HOME write
    // occurs because --dry-run skips agents_api::create.
    Case {
        description: "xvn agent create --dry-run --format json-compact (preview only)",
        args: &[
            "agent",
            "create",
            "--name",
            "dry-run-contract-agent",
            "--tools",
            "ohlcv",
            "--provider",
            "anthropic",
            "--model",
            "claude-haiku-4-5",
            "--system-prompt",
            "You are a regime filter for the trader agent. Inspect the supplied OHLCV context, recent volatility, and risk limits, and emit JSON so the downstream trader knows when to dispatch. Stay grounded.",
            "--format",
            "json-compact",
            "--dry-run",
        ],
        pass_json: false,
        extra_args: &[],
    },
];

/// Run one case and return `(stdout_bytes, stderr_string)`.
fn run_case(case: &Case, home: &std::path::Path) -> (Vec<u8>, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xvn"));
    cmd.env("XVN_HOME", home);
    cmd.env_remove("XVN_REMOTE_URL");
    cmd.args(case.args);
    cmd.args(case.extra_args);
    if case.pass_json {
        cmd.arg("--json");
    }
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawn `xvn {}`: {e}", case.args.join(" ")));
    (out.stdout, String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn every_audit_verb_produces_clean_stdout_under_json() {
    let dir = tempdir().unwrap();

    // We collect every failure first so a single test run names every
    // regression at once, per the contract acceptance.
    let mut failures: Vec<String> = Vec::new();

    for case in CASES {
        let (stdout, stderr) = run_case(case, dir.path());

        // 1. Banner-marker scan. Forbidden markers must not appear on
        //    stdout regardless of JSON-parse outcome.
        for marker in FORBIDDEN_MARKERS {
            if stdout.windows(marker.len()).any(|w| w == marker.as_bytes()) {
                failures.push(format!(
                    "[{}] forbidden marker {marker:?} on stdout\nstdout:\n{}\n--- stderr ---\n{stderr}",
                    case.description,
                    String::from_utf8_lossy(&stdout),
                ));
            }
        }

        // 2. JSON-parse check. Empty stdout is fine (verb errored out
        //    pre-render); non-empty stdout MUST be exactly one parseable
        //    JSON value.
        if !stdout.is_empty() {
            // Trim trailing whitespace (the `println!` after the JSON
            // payload is a single newline by design).
            let trimmed = trim_trailing_ws(&stdout);
            match serde_json::from_slice::<serde_json::Value>(trimmed) {
                Ok(_) => { /* good */ }
                Err(e) => {
                    failures.push(format!(
                        "[{}] stdout failed JSON parse: {e}\nstdout:\n{}\n--- stderr ---\n{stderr}",
                        case.description,
                        String::from_utf8_lossy(&stdout),
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "json-stdout-contract violated by {} case(s):\n\n{}",
            failures.len(),
            failures.join("\n\n"),
        );
    }
}

/// Additional contract check for `xvn agent create --dry-run --format
/// json-compact`: stdout must be a single line (no embedded newlines)
/// containing exactly one parseable JSON object with `"dry_run":true`
/// and a `"would_create"` key.
#[test]
fn agent_create_dry_run_json_compact_is_single_line_object() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xvn"));
    cmd.env("XVN_HOME", dir.path());
    cmd.env_remove("XVN_REMOTE_URL");
    cmd.args(&[
        "agent",
        "create",
        "--name",
        "compact-line-contract-agent",
        "--tools",
        "indicator_panel",
        "--provider",
        "openrouter",
        "--model",
        "anthropic/claude-3.5-sonnet",
        "--system-prompt",
        "You are a regime filter for the trader agent. Inspect the supplied OHLCV context, recent volatility, and risk limits, and emit JSON so the downstream trader knows when to dispatch. Stay grounded.",
        "--format",
        "json-compact",
        "--dry-run",
    ]);
    let out = cmd.output().expect("spawn xvn");
    assert_eq!(
        out.status.code().expect("signal?"),
        0,
        "--dry-run must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout.clone()).expect("stdout is UTF-8");
    let trimmed = stdout.trim_end_matches(['\n', '\r', ' ', '\t']);

    // Single-line: no embedded newlines in the JSON payload itself.
    assert!(
        !trimmed.contains('\n'),
        "json-compact stdout must be a single line; got:\n{trimmed}",
    );

    // Must parse as a JSON object.
    let val: serde_json::Value = serde_json::from_str(trimmed).expect("stdout must be valid JSON");
    assert_eq!(val["dry_run"], true, "dry_run field must be true");
    assert!(
        val.get("would_create").is_some(),
        "would_create key must be present; got: {val}"
    );
}

#[test]
fn agent_create_dry_run_long_unicode_prompt_does_not_panic() {
    let dir = tempdir().unwrap();
    let prompt = format!("{}é{}", "a".repeat(119), "b".repeat(80));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xvn"));
    cmd.env("XVN_HOME", dir.path());
    cmd.env_remove("XVN_REMOTE_URL");
    cmd.args(&[
        "agent",
        "create",
        "--name",
        "unicode-preview-agent",
        "--tools",
        "indicator_panel",
        "--provider",
        "openrouter",
        "--model",
        "anthropic/claude-3.5-sonnet",
        "--system-prompt",
        &prompt,
        "--format",
        "json-compact",
        "--dry-run",
    ]);

    let out = cmd.output().expect("spawn xvn");
    assert_eq!(
        out.status.code().expect("signal?"),
        0,
        "--dry-run must not panic on multibyte prompt truncation; stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let val: serde_json::Value = serde_json::from_str(stdout.trim_end()).expect("stdout must be valid JSON");
    assert_eq!(val["dry_run"], true);
    let preview = val["would_create"]["system_prompt_preview"]
        .as_str()
        .expect("preview must be a string");
    assert!(
        preview.contains('é'),
        "preview should preserve valid UTF-8 chars; got: {preview}"
    );
}

/// Additional contract check: `xvn agent ls --format json` on an empty
/// home emits ONLY a valid JSON array (the empty array `[]`) on stdout.
#[test]
fn agent_ls_format_json_empty_home_emits_json_array() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xvn"));
    cmd.env("XVN_HOME", dir.path());
    cmd.env_remove("XVN_REMOTE_URL");
    cmd.args(&["agent", "ls", "--format", "json"]);
    let out = cmd.output().expect("spawn xvn");
    assert_eq!(
        out.status.code().expect("signal?"),
        0,
        "agent ls on empty home must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = out.stdout.clone();
    let trimmed = trim_trailing_ws(&stdout);
    let val: serde_json::Value = serde_json::from_slice(trimmed).expect("stdout must be valid JSON");
    assert!(
        val.is_array(),
        "agent ls --format json must emit a JSON array; got: {val}",
    );
}

fn trim_trailing_ws(buf: &[u8]) -> &[u8] {
    let mut end = buf.len();
    while end > 0 && matches!(buf[end - 1], b'\n' | b'\r' | b' ' | b'\t') {
        end -= 1;
    }
    &buf[..end]
}
