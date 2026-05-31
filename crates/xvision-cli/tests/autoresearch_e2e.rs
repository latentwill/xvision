//! Autoresearcher E2E: banned operator-facing term check on CLI help output.
//!
//! Banned terms are drawn from the operator-vocabulary lock doc:
//! docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md

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
    // Source: docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md
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
         (see docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md).\n\n\
         Violations:\n{}",
        failures.join("\n")
    );
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn autoresearch_help_contains_no_banned_terms() {
    let help = xvn_help("autoresearch");
    assert_no_banned_terms("autoresearch", &help);
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

/// Verify that `xvn autoresearch --help` exits 0 and lists the core
/// operator-facing autoresearch subcommands.
#[test]
fn autoresearch_help_exits_zero_and_lists_known_subcommands() {
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["autoresearch", "--help"])
        .output()
        .expect("xvn autoresearch --help must not fail to spawn");
    assert!(
        out.status.success(),
        "xvn autoresearch --help must exit 0, got {:?}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("session-init"),
        "autoresearch --help must list session-init, got:\n{stdout}"
    );
    assert!(
        stdout.contains("mutate-once"),
        "autoresearch --help must list mutate-once, got:\n{stdout}"
    );
    assert!(
        stdout.contains("evening-cycle"),
        "autoresearch --help must list evening-cycle, got:\n{stdout}"
    );
    assert!(
        stdout.contains("demo"),
        "autoresearch --help must list demo, got:\n{stdout}"
    );
}
