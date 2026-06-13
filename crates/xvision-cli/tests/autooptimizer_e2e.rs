//! AutoOptimizer E2E: banned operator-facing term check on CLI help output.
//!
//! Banned terms are drawn from the operator-vocabulary lock doc:
//! docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md

use std::process::Command;

// ── helpers ───────────────────────────────────────────────────────────────────

fn xvn_help(subcommand: &str) -> String {
    // Use the binary that Cargo compiled for integration tests.
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args([subcommand, "--help"])
        .output()
        .unwrap_or_else(|e| panic!("failed to run `xvn {subcommand} --help`: {e}"));
    // clap writes help to stdout; collect both just in case.
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    format!("{stdout}{stderr}")
}

/// Asserts that `text` does NOT contain any of the given banned words.
/// A "word" match uses whole-word boundaries via a simple check:
/// the term must not appear as a substring surrounded only by
/// alphanumeric/underscore/hyphen characters on both sides.
///
/// For simplicity we use a case-insensitive substring search.  The banned
/// list is precise enough that false positives are very unlikely.
fn assert_no_banned_terms(surface: &str, text: &str) {
    // Terms that must never appear in operator-facing CLI help.
    // Source: docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md
    let banned: &[&str] = &[
        "promote",
        "demote",
        " epsilon",
        " holdout",
        "mutation",
        "mutator",
        "ghost",
        "quarantined",
        " merkle",
    ];

    let lower = text.to_lowercase();
    let mut failures: Vec<String> = Vec::new();

    for &term in banned {
        if lower.contains(term.to_lowercase().as_str()) {
            failures.push(format!(
                "  banned term {:?} found in `xvn {surface} --help`",
                term
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Banned operator-facing terms detected in `xvn {surface} --help`.\n\
         These terms must be replaced with their operator-vocabulary equivalents\n\
         (see docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md).\n\n\
         Violations:\n{}",
        failures.join("\n")
    );
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn autooptimizer_help_contains_no_banned_terms() {
    let help = xvn_help("optimize");
    assert_no_banned_terms("optimize", &help);
}

#[test]
fn memory_help_contains_no_banned_terms() {
    let help = xvn_help("memory");
    assert_no_banned_terms("memory", &help);
}

#[test]
fn flywheel_help_contains_no_banned_terms() {
    let help = xvn_help("flywheel");
    assert_no_banned_terms("flywheel", &help);
}

// ── compile-time sanity (always runs) ─────────────────────────────────────────

/// Sanity check that the binary is reachable and `--help` exits 0.
/// This test always runs (no `#[ignore]`) and verifies the binary compiles
/// and the top-level help flag works, without asserting anything about content.
#[test]
fn xvn_binary_is_reachable_and_help_exits_zero() {
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .arg("--help")
        .output()
        .expect("xvn --help must not fail to spawn");
    assert!(
        out.status.success(),
        "xvn --help must exit 0, got {:?}\nstdout={}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Verify that `xvn optimize --help` exits 0, lists the surviving
/// operator-facing subcommands, and does NOT expose the removed verbs.
#[test]
fn optimize_help_exits_zero_and_lists_known_subcommands() {
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["optimize", "--help"])
        .output()
        .expect("xvn optimize --help must not fail to spawn");
    assert!(
        out.status.success(),
        "xvn optimize --help must exit 0, got {:?}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Surviving verbs must appear.
    for verb in &["run", "ls", "show", "lineage", "unlock"] {
        assert!(
            stdout.contains(verb),
            "optimize --help must list {verb:?}, got:\n{stdout}"
        );
    }

    // Removed verbs must NOT appear.
    assert!(
        !stdout.contains("mutate-once"),
        "optimize --help must NOT list mutate-once (removed), got:\n{stdout}"
    );
    assert!(
        !stdout.contains("run-cycle"),
        "optimize --help must NOT list run-cycle (removed), got:\n{stdout}"
    );
    assert!(
        !stdout.contains("demo"),
        "optimize --help must NOT list demo (removed), got:\n{stdout}"
    );
}

/// GH #965/#966/#968: `xvn optimize run --help` exposes the continuous-loop and
/// live-streaming flags (`--max-cycles`, `--ipc-socket`) and HIDES the internal
/// `--mock` smoke switch from the operator surface.
#[test]
fn optimize_run_help_shows_loop_and_ipc_flags_and_hides_mock() {
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["optimize", "run", "--help"])
        .output()
        .expect("xvn optimize run --help must not fail to spawn");
    assert!(
        out.status.success(),
        "xvn optimize run --help must exit 0, got {:?}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("--max-cycles"),
        "optimize run --help must document --max-cycles (GH #965), got:\n{stdout}"
    );
    assert!(
        stdout.contains("--ipc-socket"),
        "optimize run --help must document --ipc-socket (GH #968), got:\n{stdout}"
    );
    // --mock is internal/CI only and hidden from the operator help surface (GH #966).
    assert!(
        !stdout.contains("--mock"),
        "optimize run --help must NOT expose the hidden --mock flag, got:\n{stdout}"
    );
}
