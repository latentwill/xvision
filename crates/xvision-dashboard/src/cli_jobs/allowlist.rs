//! Allowlist of safe CLI job templates served over `POST /api/cli/jobs`.
//!
//! Per QA 2026-05-17 finding #3 (`qa-dashboard-auth-hardening`), the
//! previous validation surface relied on a small denylist of obviously
//! dangerous subcommands (`dashboard`, `mcp`). The problem with a
//! denylist is its open-world default: anything not denied is allowed,
//! and as the CLI grows new subcommands (e.g. `fire-trade`, provider
//! mutation, destructive settings) silently become reachable from the
//! HTTP surface.
//!
//! This module flips the default to allowlist. Today exactly one
//! template is permitted: `bars fetch` (the per-scenario "fetch missing
//! bars" panel in the dashboard). Adding a new template is a deliberate
//! review act — update the table below.
//!
//! ## Operator-mode opt-out
//!
//! For local development a permissive mode is available via the
//! `XVN_DASHBOARD_CLI_DEVMODE` env var. When set to `1`, this module
//! falls back to the pre-existing denylist behavior (only `dashboard`
//! and `mcp` are explicitly rejected). The mode is **not** a substitute
//! for the auth gate — non-loopback binds still require the configured
//! shared secret regardless of CLI devmode.

/// Verdict from a single allowlist check. The string is shown verbatim
/// in the dashboard's HTTP error so the operator can diagnose why a
/// job was rejected.
pub enum AllowlistDecision {
    Allow,
    Reject(String),
}

/// Single allowlist entry. `head` is the prefix the argv must start
/// with (e.g. `["bars", "fetch"]`). `permitted_flags` lists the long
/// flags that may follow; the validator scans alternating flag/value
/// pairs after `head` and rejects anything not in this set.
struct Template {
    head: &'static [&'static str],
    permitted_flags: &'static [&'static str],
}

const TEMPLATES: &[Template] = &[Template {
    head: &["bars", "fetch"],
    permitted_flags: &["--asset", "--granularity", "--from", "--to"],
}];

const DENYLIST_SUBCOMMANDS: &[&str] = &["dashboard", "mcp", "fire-trade"];

const DEVMODE_ENV: &str = "XVN_DASHBOARD_CLI_DEVMODE";

/// Check argv against the allowlist. Empty argv is the caller's
/// concern (the route validates that separately) — this function
/// assumes at least one element.
pub fn check_argv(argv: &[String]) -> AllowlistDecision {
    if argv.is_empty() {
        return AllowlistDecision::Reject("argv is empty".into());
    }

    // Hard deny: a small set of subcommands that are categorically
    // unsafe to expose over the HTTP surface regardless of mode.
    // Mirrors and extends the previous denylist on `routes::cli`.
    let head = argv[0].as_str();
    if DENYLIST_SUBCOMMANDS.iter().any(|d| *d == head) {
        return AllowlistDecision::Reject(format!("subcommand `{head}` is not allowed over remote cli"));
    }

    if devmode_enabled() {
        return AllowlistDecision::Allow;
    }

    for tmpl in TEMPLATES {
        if argv_matches(argv, tmpl) {
            return AllowlistDecision::Allow;
        }
    }

    AllowlistDecision::Reject(format!(
        "argv does not match any allowlisted template; \
         set {DEVMODE_ENV}=1 on the dashboard process to bypass for local development"
    ))
}

fn argv_matches(argv: &[String], tmpl: &Template) -> bool {
    if argv.len() < tmpl.head.len() {
        return false;
    }
    for (got, want) in argv.iter().zip(tmpl.head.iter()) {
        if got.as_str() != *want {
            return false;
        }
    }

    // Walk the remaining args as `--flag value` pairs. Any flag
    // outside `permitted_flags` is a reject.
    let mut idx = tmpl.head.len();
    while idx < argv.len() {
        let flag = argv[idx].as_str();
        if !flag.starts_with("--") {
            return false;
        }
        if !tmpl.permitted_flags.iter().any(|f| *f == flag) {
            return false;
        }
        // Flag must have a value — refuse a trailing flag with no
        // pair so the caller can't smuggle in side-effects via
        // adjacent option parsing.
        if idx + 1 >= argv.len() {
            return false;
        }
        idx += 2;
    }
    true
}

fn devmode_enabled() -> bool {
    std::env::var(DEVMODE_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn assert_allow(parts: &[&str]) {
        match check_argv(&argv(parts)) {
            AllowlistDecision::Allow => {}
            AllowlistDecision::Reject(msg) => panic!("expected allow, got reject: {msg}"),
        }
    }

    fn assert_reject(parts: &[&str], expected_hint: &str) {
        match check_argv(&argv(parts)) {
            AllowlistDecision::Allow => panic!("expected reject for argv {parts:?}"),
            AllowlistDecision::Reject(msg) => assert!(
                msg.contains(expected_hint),
                "reject message `{msg}` should mention `{expected_hint}`",
            ),
        }
    }

    #[test]
    fn bars_fetch_with_full_argv_is_allowed() {
        let _guard = clear_devmode();
        assert_allow(&[
            "bars",
            "fetch",
            "--asset",
            "BTC/USD",
            "--granularity",
            "1h",
            "--from",
            "2025-01-01",
            "--to",
            "2025-02-01",
        ]);
    }

    #[test]
    fn bars_fetch_partial_is_allowed_as_long_as_flags_are_permitted() {
        let _guard = clear_devmode();
        assert_allow(&["bars", "fetch", "--asset", "BTC/USD"]);
    }

    #[test]
    fn bars_fetch_with_unknown_flag_is_rejected() {
        let _guard = clear_devmode();
        assert_reject(
            &["bars", "fetch", "--asset", "BTC/USD", "--force", "true"],
            "allowlisted template",
        );
    }

    #[test]
    fn bars_fetch_with_dangling_flag_is_rejected() {
        let _guard = clear_devmode();
        assert_reject(&["bars", "fetch", "--asset"], "allowlisted template");
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let _guard = clear_devmode();
        assert_reject(&["eval", "run", "--strategy", "abc"], "allowlisted template");
    }

    #[test]
    fn dashboard_subcommand_is_always_rejected_even_in_devmode() {
        let _guard = set_devmode("1");
        assert_reject(&["dashboard", "serve"], "not allowed over remote cli");
    }

    #[test]
    fn mcp_subcommand_is_always_rejected_even_in_devmode() {
        let _guard = set_devmode("1");
        assert_reject(&["mcp", "stdio"], "not allowed over remote cli");
    }

    #[test]
    fn fire_trade_subcommand_is_always_rejected() {
        let _guard = clear_devmode();
        assert_reject(&["fire-trade"], "not allowed over remote cli");
    }

    #[test]
    fn devmode_allows_arbitrary_subcommand() {
        let _guard = set_devmode("1");
        assert_allow(&["eval", "run", "--strategy", "abc"]);
    }

    #[test]
    fn devmode_off_falls_back_to_strict_allowlist() {
        let _guard = clear_devmode();
        assert_reject(&["eval", "run"], "allowlisted template");
    }

    /// Tests mutate process env (`DEVMODE_ENV`); serialize via a
    /// process-wide mutex so cargo's default parallel runner doesn't
    /// race two tests on the same global. Cheaper than pulling in
    /// `serial_test` for a five-test module.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        prev: Option<String>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var(DEVMODE_ENV, v),
                None => std::env::remove_var(DEVMODE_ENV),
            }
        }
    }

    fn set_devmode(v: &str) -> EnvGuard {
        let lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var(DEVMODE_ENV).ok();
        std::env::set_var(DEVMODE_ENV, v);
        EnvGuard { prev, _lock: lock }
    }

    fn clear_devmode() -> EnvGuard {
        let lock = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var(DEVMODE_ENV).ok();
        std::env::remove_var(DEVMODE_ENV);
        EnvGuard { prev, _lock: lock }
    }
}
