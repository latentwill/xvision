//! Remote CLI job policy served over `POST /api/cli/jobs`.
//!
//! The HTTP surface accepts typed argv only — no shell text, no caller-controlled
//! cwd, and no caller-controlled env. That keeps command injection out of scope,
//! but the route still needs an application-level policy so server-style or
//! live-trading commands cannot be triggered from the dashboard job API.
//!
//! ## Safe-to-surface principle
//!
//! A command is safe to surface remotely when it meets ONE of these criteria:
//!   (a) **Read-only**: it cannot mutate persistent state (e.g. `eval list`,
//!       `strategy show`, `scenario show`).
//!   (b) **Explicitly scoped + hard-limited + cancellable**: it accepts a
//!       mandatory scope argument (e.g. a strategy or scenario ID), the engine
//!       enforces hard caps on decisions/tokens/wall-clock (PR #428), and the
//!       dashboard can cancel it via `DELETE /api/cli/jobs/:id` (this track).
//!       Examples: `eval run`, `eval compare`, `eval watch`, bounded variants
//!       of `experiment run` / `model bakeoff`.
//!
//! Verbs that haven't met that bar (mutating state without a scope, spawning
//! server processes, firing live trades) stay off the allowlist regardless of
//! how convenient they would be to surface.
//!
//! This allowlist was expanded in `v2b-remote-cli-job-safety` to fold in the
//! operator-safety P1 #12 safe-eval verbs from
//! `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`.
//!
//! By default the policy is an operator command allowlist: normal
//! read/eval/research commands work on live nodes with no flag, and
//! categorically dangerous heads stay denied. Setting `XVN_DASHBOARD_CLI_DEVMODE`
//! (`1`/`true`) turns the policy into a FULL bypass — every argv is accepted,
//! including live-trade and host-admin verbs. That flag is an explicit per-node
//! opt-in for a trusted dev tailnet with no live broker credentials; it does
//! not replace the (pending) dashboard auth gate. See [`check_argv_with_env`].

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
    permitted_switches: &'static [&'static str],
}

struct DeniedNested {
    head: &'static str,
    path: &'static [&'static str],
}

/// Strict templates for commands that are safe remotely only when called with
/// a constrained flag set. Each template specifies the exact subcommand prefix
/// that must appear (e.g. `["bars", "fetch"]`) and the exhaustive list of flags
/// that are permitted after that prefix.
///
/// Read-only commands (like `eval list` or `strategy show`) do not need a
/// strict template — they are covered by the SUPPORTED_SUBCOMMANDS allowlist
/// plus the DENIED_NESTED_SUBCOMMANDS denylist. Strict templates are only used
/// for commands that are safe when scoped but dangerous without constraints.
const STRICT_TEMPLATES: &[Template] = &[
    // Data fetch — safe remotely, constrained flag set.
    Template {
        head: &["bars", "fetch"],
        permitted_flags: &["--asset", "--granularity", "--from", "--to"],
        permitted_switches: &[],
    },
    // Bounded experiment run — safe when a specific experiment ID is provided
    // and the engine enforces hard caps (PR #428: --max-decisions,
    // --max-input-tokens, --max-output-tokens, --max-wall-clock).
    // The dashboard supervisor adds the runtime-cap and output-cap layer
    // on top of the engine's per-run budgets.
    Template {
        head: &["experiment", "run"],
        permitted_flags: &[
            "--id",
            "--strategy",
            "--scenario",
            "--mode",
            "--max-decisions",
            "--max-input-tokens",
            "--max-output-tokens",
            "--max-wall-clock",
            "--arm",
            "--cycles",
            "--tag",
        ],
        permitted_switches: &["--cancel-on-token-limit"],
    },
    // Bounded model bakeoff — safe when scoped to a specific strategy +
    // scenario set. Same cap story as experiment run.
    Template {
        head: &["model", "bakeoff"],
        permitted_flags: &[
            // Selection
            "--strategy",
            "--strategies",
            "--scenario",
            "--provider",
            "--models",
            // Materialization
            "--mode",
            "--clone-name-template",
            "--name",
            // Execution shape (sequential default; parallel opt-in)
            "--max-runs",
            "--run-mode",
            // Hard limits (PR #428)
            "--max-decisions",
            "--max-input-tokens",
            "--max-output-tokens",
            "--max-wall-clock",
            // Output
            // Legacy / forward-compat keys from V2B remote-cli-job-safety
            "--arm",
            "--cycles",
            "--tag",
            "--compare-with",
        ],
        permitted_switches: &[
            "--use-strategy-models",
            "--sequential",
            "--parallel",
            "--wait",
            "--cancel-on-token-limit",
            "--compare",
            "--markdown",
            "--json",
            "--yes",
        ],
    },
    // UI/agent-launched optimizer. The dashboard job supervisor makes it
    // cancellable and caps runtime/output. The cycle verb is `xvn optimize run`
    // — the SAME one operators use — so CLI runs and the dashboard converge on a
    // single path (GH #965/#968). `--max-cycles` bounds a multi-cycle run;
    // `--ipc-socket` streams live progress into the dashboard; `--mock` is an
    // optional internal smoke switch.
    Template {
        head: &["optimize", "run"],
        permitted_flags: &[
            "--session-id",
            "--config",
            "--db",
            "--strategy",
            "--budget",
            "--provider",
            "--model",
            "--objective",
            "--max-cycles",
            "--ipc-socket",
        ],
        permitted_switches: &["--mock"],
    },
];

/// Top-level commands that should never be reachable through the remote CLI
/// job API, even though they may be legitimate local `xvn` commands.
const DENYLIST_SUBCOMMANDS: &[&str] = &[
    "dashboard",      // starts another HTTP server
    "mcp",            // starts an MCP server/session
    "fire-trade",     // explicit live order smoke test
    "close-position", // explicit live position mutation
    "init",           // initializes/migrates the dashboard host DB (alias: migrate)
];

/// Top-level commands that are supported through the remote CLI job API.
/// Command-specific validation can still reject a supported head below.
///
/// Safe-to-surface principle (see module doc): a verb is listed here when it
/// is read-only OR when it is explicitly scoped + hard-limited + cancellable.
const SUPPORTED_SUBCOMMANDS: &[&str] = &[
    "--help",
    "-h",
    "--version",
    "-V",
    "help",
    "agent",
    "bars",
    "doctor",
    "eod",
    "eval", // eval list, eval show, eval results, eval watch,
    // eval compare, eval cancel — all read-only or cancellable
    "example",
    "experiment", // bounded via STRICT_TEMPLATES (experiment run only)
    "gate",
    "indicator",
    "metrics",
    // "init" is in DENYLIST_SUBCOMMANDS — intentionally absent here
    "model", // bounded model bakeoff via STRICT_TEMPLATES
    "obs",
    "optimize", // bounded via STRICT_TEMPLATES (optimize run only)
    "portfolio",
    "provider",
    "report",
    // `risk` was retired from the CLI in #1038 (Byreal/perps risk unified onto
    // the engine veto path); this stale allowlist entry pointed at a removed
    // verb and tripped `remote_allowlist_paths_exist_in_clap_tree`.
    "run",
    "scenario", // scenario show, scenario select — read-only paths allowed
    "show-decision",
    "show-metrics",
    "store",
    "strategy", // strategy show, strategy validate — read-only paths allowed
];

/// Mutating, destructive, or host-admin paths below otherwise-supported heads.
const DENIED_NESTED_SUBCOMMANDS: &[DeniedNested] = &[
    // `agent create` is a write path: persists a new agent record in the
    // workspace library. Read-only agent verbs (get, ls, lint) remain
    // allowed. Added with the agent dry-run + allowlist gap fix.
    DeniedNested {
        head: "agent",
        path: &["create"],
    },
    DeniedNested {
        head: "bars",
        path: &["rm"],
    },
    DeniedNested {
        head: "bars",
        path: &["gc"],
    },
    DeniedNested {
        head: "provider",
        path: &["add"],
    },
    DeniedNested {
        head: "provider",
        path: &["remove"],
    },
    DeniedNested {
        head: "provider",
        path: &["refresh-models"],
    },
    DeniedNested {
        head: "scenario",
        path: &["create"],
    },
    DeniedNested {
        head: "scenario",
        path: &["clone"],
    },
    DeniedNested {
        head: "scenario",
        path: &["archive"],
    },
    DeniedNested {
        head: "scenario",
        path: &["rm"],
    },
    DeniedNested {
        head: "scenario",
        path: &["classify"],
    },
    DeniedNested {
        head: "scenario",
        path: &["set-regime"],
    },
    // Strategy draft authoring is intentionally allowed over the trusted
    // Tailscale remote CLI: users need to create new strategies remotely
    // without bouncing through the dashboard UI. The mutating follow-up paths
    // below (`add-agent`, `remove-agent`, `set-pipeline`) stay denied because
    // they alter an existing strategy's shape in ways that are easier to do
    // with richer local context.
    DeniedNested {
        head: "strategy",
        path: &["add-agent"],
    },
    DeniedNested {
        head: "strategy",
        path: &["remove-agent"],
    },
    DeniedNested {
        head: "strategy",
        path: &["set-pipeline"],
    },
    DeniedNested {
        head: "strategy",
        path: &["migrate-agents"],
    },
    DeniedNested {
        head: "experiment",
        path: &["new"],
    },
    DeniedNested {
        head: "experiment",
        path: &["create"],
    },
    DeniedNested {
        head: "experiment",
        path: &["update"],
    },
    DeniedNested {
        head: "example",
        path: &["seed"],
    },
    DeniedNested {
        head: "obs",
        path: &["retention", "set"],
    },
    DeniedNested {
        head: "obs",
        path: &["retention", "clear"],
    },
    DeniedNested {
        head: "obs",
        path: &["janitor", "run"],
    },
    DeniedNested {
        head: "store",
        path: &["migrate"],
    },
];

/// Every real command/subcommand path the remote policy *references*
/// (allowed heads, strict-template heads, allowed draft-authoring paths,
/// and nested-denied paths), for the CLI-surface drift test. Excludes pseudo-flags
/// (--help/-h/--version/-V/help) and the defensive DENYLIST (which may name
/// commands that don't exist as xvn subcommands on purpose).
pub fn referenced_command_paths() -> Vec<Vec<&'static str>> {
    let mut paths: Vec<Vec<&'static str>> = Vec::new();
    for t in STRICT_TEMPLATES {
        paths.push(t.head.to_vec());
    }
    for s in SUPPORTED_SUBCOMMANDS {
        if s.starts_with('-') || *s == "help" {
            continue;
        }
        paths.push(vec![*s]);
    }
    // Draft-authoring paths are intentionally allowed even though they mutate
    // state, so keep them visible to the clap-drift test as first-class remote
    // policy entries.
    paths.push(vec!["strategy", "create"]);
    paths.push(vec!["strategy", "new"]);
    for d in DENIED_NESTED_SUBCOMMANDS {
        let mut p = vec![d.head];
        p.extend_from_slice(d.path);
        paths.push(p);
    }
    paths
}

/// Check argv against the remote CLI policy. Empty argv is the caller's
/// concern (the route validates that separately) — this function
/// assumes at least one element.
pub fn check_argv(argv: &[String]) -> AllowlistDecision {
    if argv.is_empty() {
        return AllowlistDecision::Reject("argv is empty".into());
    }

    let head = argv[0].as_str();
    if DENYLIST_SUBCOMMANDS.contains(&head) {
        return AllowlistDecision::Reject(format!("subcommand `{head}` is not allowed over remote cli"));
    }

    if !SUPPORTED_SUBCOMMANDS.contains(&head) {
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
    } else if head == "optimize" {
        return AllowlistDecision::Reject(
            "subcommand `optimize` is only allowed over remote cli as `optimize run`".into(),
        );
    }

    AllowlistDecision::Allow
}

/// Env var that opts a node into the remote CLI full-bypass dev mode.
pub const DEVMODE_ENV: &str = "XVN_DASHBOARD_CLI_DEVMODE";

/// Whether the full-bypass dev mode is enabled on this node. Off unless the
/// env var is `1` or (case-insensitively) `true`.
pub fn devmode_enabled() -> bool {
    std::env::var(DEVMODE_ENV)
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

/// Policy entry point for the HTTP route.
///
/// With dev mode **off** (the default, and the only safe posture for any node
/// reachable beyond a trusted tailnet) this is exactly [`check_argv`]: the
/// allowlist + denylist apply.
///
/// With dev mode **on** it is a FULL bypass — every non-empty argv is allowed,
/// including the live-trade (`fire-trade`, `close-position`) and host-admin
/// (`init`, `dashboard`, `mcp`) verbs that [`check_argv`] denies. This is an
/// explicit per-node opt-in (set [`DEVMODE_ENV`] only on a trusted dev node
/// with no live broker credentials); it does NOT replace the (pending) auth
/// gate.
pub fn check_argv_with_env(argv: &[String]) -> AllowlistDecision {
    if devmode_enabled() && !argv.is_empty() {
        return AllowlistDecision::Allow;
    }
    check_argv(argv)
}

fn denied_nested_subcommand(argv: &[String]) -> Option<String> {
    let head = argv.first()?.as_str();
    for denied in DENIED_NESTED_SUBCOMMANDS {
        if denied.head == head && argv_matches_path(argv, denied.path) {
            let path = std::iter::once(head)
                .chain(denied.path.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            return Some(format!("subcommand `{path}` is not allowed over remote cli"));
        }
    }
    None
}

fn argv_matches_path(argv: &[String], path: &[&str]) -> bool {
    if argv.len() < path.len() + 1 {
        return false;
    }
    argv.iter()
        .skip(1)
        .zip(path.iter())
        .all(|(got, want)| got.as_str() == *want)
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
        if tmpl.permitted_switches.contains(&flag) {
            idx += 1;
            continue;
        }
        if !tmpl.permitted_flags.contains(&flag) {
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
    fn optimizer_run_mock_is_allowed() {
        assert_allow(&[
            "optimize",
            "run",
            "--session-id",
            "ui-01",
            "--mock",
            "--strategy",
            "st_1",
            "--budget",
            "5.00",
        ]);
    }

    #[test]
    fn optimizer_run_non_mock_is_allowed() {
        assert_allow(&[
            "optimize",
            "run",
            "--session-id",
            "ui-01",
            "--strategy",
            "st_1",
            "--budget",
            "5.00",
        ]);
    }

    #[test]
    fn optimizer_run_continuous_and_ipc_are_allowed() {
        // GH #965/#968: the unified verb accepts --max-cycles (multi-cycle /
        // fire-and-forget) and --ipc-socket (live dashboard streaming).
        assert_allow(&[
            "optimize",
            "run",
            "--strategy",
            "st_1",
            "--max-cycles",
            "0",
            "--ipc-socket",
            "/tmp/xvn-optimizer.sock",
        ]);
    }

    #[test]
    fn optimizer_other_subcommands_are_rejected() {
        // `optimize` (the consolidated cycle verb) is only remotely launchable
        // as `optimize run`; a bare `optimize` and any other subcommand are
        // rejected.
        assert_reject(
            &["optimize", "memory-demos", "--agent", "ag_1"],
            "only allowed over remote cli as `optimize run`",
        );
    }

    #[test]
    fn optimizer_run_unknown_flag_is_rejected() {
        assert_reject(
            &[
                "optimize",
                "run",
                "--session-id",
                "ui-01",
                "--real-broker",
                "true",
            ],
            "supported remote cli template",
        );
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
    fn strategy_and_scenario_read_paths_are_allowed_without_devmode() {
        assert_allow(&["strategy", "show", "st_1"]);
        assert_allow(&["strategy", "validate", "st_1", "--scenario", "sc_1"]);
        assert_allow(&["scenario", "show", "sc_1"]);
        assert_allow(&["scenario", "select", "--asset", "BTC/USD", "--count", "4"]);
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
        assert_reject(
            &["close-position", "--asset", "BTC/USD"],
            "not allowed over remote cli",
        );
    }

    #[test]
    fn destructive_nested_commands_are_rejected() {
        assert_reject(
            &["bars", "rm", "--asset", "BTC/USD"],
            "not allowed over remote cli",
        );
        assert_reject(&["bars", "gc"], "not allowed over remote cli");
        assert_reject(
            &["provider", "remove", "--name", "openrouter"],
            "not allowed over remote cli",
        );
        assert_reject(&["provider", "add", "--name", "x"], "not allowed over remote cli");
        assert_reject(&["provider", "refresh-models"], "not allowed over remote cli");
    }

    #[test]
    fn authoring_and_admin_nested_commands_are_rejected() {
        for parts in [
            &["scenario", "create", "--name", "remote-test"][..],
            &["scenario", "clone", "sc_1", "--name", "copy"][..],
            &["scenario", "archive", "sc_1"][..],
            &["scenario", "rm", "sc_1"][..],
            &["scenario", "classify", "--all"][..],
            &["scenario", "set-regime", "sc_1", "--regime", "trend"][..],
            &["strategy", "add-agent", "st_1", "ag_1", "--role", "trader"][..],
            &["strategy", "remove-agent", "st_1", "--role", "trader"][..],
            &["strategy", "set-pipeline", "st_1", "--kind", "single"][..],
            &["strategy", "migrate-agents"][..],
            &["experiment", "new", "--name", "remote-test"][..],
            &["experiment", "create", "--name", "remote-test"][..],
            &["experiment", "update", "exp_1", "--conclusion", "done"][..],
            &["example", "seed"][..],
            &["obs", "retention", "set", "--mode", "full-debug"][..],
            &["obs", "retention", "clear"][..],
            &["obs", "janitor", "run"][..],
            &["store", "migrate"][..],
        ] {
            assert_reject(parts, "not allowed over remote cli");
        }
    }

    #[test]
    fn top_level_init_is_rejected() {
        assert_reject(&["init", "--dry-run"], "not allowed over remote cli");
    }

    // ── write-path gaps we still want blocked remotely ───────────────────

    #[test]
    fn agent_create_is_rejected_remotely() {
        assert_reject(
            &["agent", "create", "--name", "remote-agent"],
            "not allowed over remote cli",
        );
    }

    #[test]
    fn strategy_create_is_allowed_remotely() {
        assert_allow(&["strategy", "create", "--name", "remote-strategy"]);
        assert_allow(&["strategy", "new", "--name", "remote-strategy"]);
    }

    #[test]
    fn agent_read_only_paths_are_allowed() {
        // get (alias show) — reads a single agent by id
        assert_allow(&["agent", "get", "ag_1"]);
        // ls (alias list) — read-only listing
        assert_allow(&["agent", "ls"]);
        // lint — read-only diagnostic scan
        assert_allow(&["agent", "lint"]);
    }
}
