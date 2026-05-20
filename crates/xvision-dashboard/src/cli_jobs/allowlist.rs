//! Remote CLI job policy served over `POST /api/cli/jobs`.
//!
//! The HTTP surface accepts typed argv only — no shell text, no caller-controlled
//! cwd, and no caller-controlled env. That keeps command injection out of scope,
//! but the route still needs an application-level policy so server-style or
//! live-trading commands cannot be triggered from the dashboard job API.
//!
//! The policy is intentionally an operator command allowlist, not a dev-mode
//! bypass. Normal read/eval/research commands work on live nodes without setting
//! `XVN_DASHBOARD_CLI_DEVMODE`; categorically dangerous heads stay denied.

/// Verdict from a single allowlist check. The string is shown verbatim
/// in the dashboard's HTTP error so the operator can diagnose why a
/// job was rejected.
pub enum AllowlistDecision {
    Allow,
    Reject(String),
}

/// Single strict template. `head` is the prefix the argv must start
/// with (e.g. `["bars", "fetch"]`). `permitted_flags` lists the long
/// flags that may follow; the validator scans alternating flag/value
/// pairs after `head` and rejects anything not in this set.
struct Template {
    head: &'static [&'static str],
    permitted_flags: &'static [&'static str],
}

const STRICT_TEMPLATES: &[Template] = &[Template {
    head: &["bars", "fetch"],
    permitted_flags: &["--asset", "--granularity", "--from", "--to"],
}];

/// Top-level commands that should never be reachable through the remote CLI
/// job API, even though they may be legitimate local `xvn` commands.
const DENYLIST_SUBCOMMANDS: &[&str] = &[
    "dashboard",      // starts another HTTP server
    "mcp",            // starts an MCP server/session
    "fire-trade",     // explicit live order smoke test
    "close-position", // explicit live position mutation
];

/// Top-level commands that are supported through the remote CLI job API.
/// Command-specific validation can still reject a supported head below.
const SUPPORTED_SUBCOMMANDS: &[&str] = &[
    "--help",
    "-h",
    "--version",
    "-V",
    "help",
    "ab-compare",
    "agent",
    "bars",
    "doctor",
    "eod",
    "eval",
    "example",
    "experiment",
    "gate",
    "indicator",
    "intern",
    "metrics",
    "migrate",
    "obs",
    "portfolio",
    "provider",
    "report",
    "risk",
    "run",
    "run-setup",
    "scenario",
    "show-briefing",
    "show-decision",
    "show-metrics",
    "store",
    "strategy",
    "trader",
];

/// Mutating or destructive subcommands below otherwise-supported heads.
const DENIED_NESTED_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("bars", &["rm", "gc"]),
    ("provider", &["add", "remove", "refresh-models"]),
];

/// Check argv against the remote CLI policy. Empty argv is the caller's
/// concern (the route validates that separately) — this function
/// assumes at least one element.
pub fn check_argv(argv: &[String]) -> AllowlistDecision {
    if argv.is_empty() {
        return AllowlistDecision::Reject("argv is empty".into());
    }

    let head = argv[0].as_str();
    if DENYLIST_SUBCOMMANDS.iter().any(|d| *d == head) {
        return AllowlistDecision::Reject(format!(
            "subcommand `{head}` is not allowed over remote cli"
        ));
    }

    if !SUPPORTED_SUBCOMMANDS.iter().any(|cmd| *cmd == head) {
        return AllowlistDecision::Reject(format!(
            "subcommand `{head}` is not a supported remote cli subcommand"
        ));
    }

    if let Some(msg) = denied_nested_subcommand(argv) {
        return AllowlistDecision::Reject(msg);
    }

    if let Some(template) = matching_strict_template_head(argv) {
        if !argv_matches(argv, template) {
            return AllowlistDecision::Reject(format!(
                "argv for `{}` must match the supported remote cli template",
                template.head.join(" ")
            ));
        }
    }

    AllowlistDecision::Allow
}

fn denied_nested_subcommand(argv: &[String]) -> Option<String> {
    let head = argv.first()?.as_str();
    let nested = argv.get(1)?.as_str();
    for (command, denied) in DENIED_NESTED_SUBCOMMANDS {
        if *command == head && denied.iter().any(|d| *d == nested) {
            return Some(format!(
                "subcommand `{head} {nested}` is not allowed over remote cli"
            ));
        }
    }
    None
}

fn matching_strict_template_head(argv: &[String]) -> Option<&'static Template> {
    STRICT_TEMPLATES.iter().find(|tmpl| {
        argv.len() >= tmpl.head.len()
            && argv
                .iter()
                .zip(tmpl.head.iter())
                .all(|(got, want)| got.as_str() == *want)
    })
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
        assert_allow(&["bars", "fetch", "--asset", "BTC/USD"]);
    }

    #[test]
    fn bars_fetch_with_unknown_flag_is_rejected() {
        assert_reject(
            &["bars", "fetch", "--asset", "BTC/USD", "--force", "true"],
            "supported remote cli template",
        );
    }

    #[test]
    fn bars_fetch_with_dangling_flag_is_rejected() {
        assert_reject(&["bars", "fetch", "--asset"], "supported remote cli template");
    }

    #[test]
    fn eval_run_is_allowed_without_devmode() {
        assert_allow(&[
            "eval",
            "run",
            "--strategy",
            "abc",
            "--scenario",
            "sc_1",
            "--mode",
            "backtest",
        ]);
    }

    #[test]
    fn strategy_and_scenario_authoring_are_allowed_without_devmode() {
        assert_allow(&[
            "strategy",
            "new",
            "--name",
            "remote-test",
            "--template",
            "mean_reversion",
        ]);
        assert_allow(&["scenario", "clone", "sc_1", "--name", "copy"]);
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        assert_reject(&["not-a-command"], "not a supported remote cli subcommand");
    }

    #[test]
    fn help_and_doctor_are_allowed() {
        assert_allow(&["help"]);
        assert_allow(&["doctor", "--json"]);
    }

    #[test]
    fn dashboard_subcommand_is_rejected() {
        assert_reject(&["dashboard", "serve"], "not allowed over remote cli");
    }

    #[test]
    fn mcp_subcommand_is_rejected() {
        assert_reject(&["mcp", "stdio"], "not allowed over remote cli");
    }

    #[test]
    fn fire_trade_subcommand_is_rejected() {
        assert_reject(&["fire-trade"], "not allowed over remote cli");
    }

    #[test]
    fn close_position_subcommand_is_rejected() {
        assert_reject(&["close-position", "--asset", "BTC/USD"], "not allowed over remote cli");
    }

    #[test]
    fn destructive_nested_commands_are_rejected() {
        assert_reject(&["bars", "rm", "--asset", "BTC/USD"], "not allowed over remote cli");
        assert_reject(&["provider", "remove", "--name", "openrouter"], "not allowed over remote cli");
        assert_reject(&["provider", "add", "--name", "x"], "not allowed over remote cli");
    }
}
