//! Focused RED coverage for Task 4 Route Builder CLI verbs.
//!
//! These tests exercise the operator-facing subprocess contract for
//! `xvn strategy route ...`: stdin/file-safe route setup, dry-run validation,
//! JSON readiness output, diagnostic errors, and no partial persistence.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::json;
use tempfile::tempdir;
use ulid::Ulid;
use xvision_engine::agents::Capability;
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
use xvision_engine::strategies::{
    ActivationMode, AgentRef, PipelineDef, PipelineKind, RouteBranch, RouteContextField, RouteDefinition,
    RouteTraceMode, Strategy,
};

fn xvn(args: &[&str], home: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn xvn_with_stdin(args: &[&str], home: &Path, stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xvn");

    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin.as_bytes())
        .expect("write route JSON to stdin");

    child.wait_with_output().expect("xvn output")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

fn seed_route_strategy(
    home: &Path,
    name: &str,
    agents: &[(&str, Capability)],
    route: Option<RouteDefinition>,
) -> String {
    let strategy_id = Ulid::new().to_string();
    let pipeline_kind = if route.is_some() {
        PipelineKind::Graph
    } else {
        PipelineKind::Sequential
    };
    let strategy = Strategy {
        manifest: PublicManifest {
            id: strategy_id.clone(),
            display_name: name.into(),
            plain_summary: "Route Builder CLI fixture".into(),
            creator: "@route-cli-test".into(),
            template: "custom".into(),
            regime_fit: vec![],
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        agents: agents
            .iter()
            .map(|(role, activates)| AgentRef {
                agent_id: Ulid::new().to_string(),
                role: (*role).into(),
                activates: Some(*activates),
                prompt: String::new(),
                model_override: None,
                checkpoint: None,
                veto: None,
            })
            .collect(),
        pipeline: PipelineDef {
            kind: pipeline_kind,
            edges: vec![],
            route,
        },
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        hypothesis: None,
        activation_mode: ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    };

    let home = home.to_path_buf();
    let id = strategy_id.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let store = FilesystemStore::new(strategy_store_dir(&home));
        store.save(&strategy).await.unwrap();
    });

    id
}

fn load_strategy(home: &Path, strategy_id: &str) -> Strategy {
    let home = home.to_path_buf();
    let strategy_id = strategy_id.to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let store = FilesystemStore::new(strategy_store_dir(&home));
        store.load(&strategy_id).await.unwrap()
    })
}

fn route_builder_agents() -> Vec<(&'static str, Capability)> {
    vec![
        ("router", Capability::Router),
        ("trend_trader", Capability::Trader),
        ("range_trader", Capability::Trader),
    ]
}

fn route_builder_agents_with_regime_filter() -> Vec<(&'static str, Capability)> {
    vec![
        ("regime_filter", Capability::Filter),
        ("router", Capability::Router),
        ("trend_trader", Capability::Trader),
        ("range_trader", Capability::Trader),
    ]
}

fn valid_route_json() -> String {
    json!({
        "router_role": " Router ",
        "branches": [
            { "target_role": " Trend_Trader " },
            { "target_role": "range_trader" }
        ],
        "graph_edges": [],
        "context_fields": [],
        "trace_mode": "compact"
    })
    .to_string()
}

fn current_route() -> RouteDefinition {
    RouteDefinition {
        router_role: "router".into(),
        branches: vec![RouteBranch {
            target_role: "trend_trader".into(),
        }],
        graph_edges: Vec::new(),
        context_fields: vec![
            RouteContextField::MarketSnapshot,
            RouteContextField::AvailableTargets,
        ],
        trace_mode: RouteTraceMode::Compact,
    }
}

fn assert_json_success(out: &std::process::Output) -> serde_json::Value {
    assert_eq!(
        code(out),
        0,
        "expected xvn command to succeed; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout must be JSON; stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn assert_json_route_error(
    out: &std::process::Output,
    strategy_id: &str,
    expected_message_fragment: &str,
) -> serde_json::Value {
    assert_ne!(
        code(out),
        0,
        "invalid route command must fail; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "--json route errors must be machine-readable JSON on stdout; stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert_eq!(body["strategy_id"], strategy_id);
    let errors = body["errors"].as_array().expect("errors must be an array");
    assert!(
        errors.iter().any(|error| {
            error
                .as_str()
                .is_some_and(|message| message.contains(expected_message_fragment))
                || error
                    .get("message")
                    .and_then(|message| message.as_str())
                    .is_some_and(|message| message.contains(expected_message_fragment))
        }),
        "expected route diagnostics to include `{expected_message_fragment}`; body={body:#}",
    );
    body
}

#[test]
fn strategy_route_setup_from_stdin_persists_normalized_route() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(dir.path(), "route-setup-stdin", &route_builder_agents(), None);

    let out = xvn_with_stdin(
        &[
            "strategy",
            "route",
            "setup",
            &strategy_id,
            "--from-stdin",
            "--json",
        ],
        dir.path(),
        &valid_route_json(),
    );

    let body = assert_json_success(&out);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(body["strategy"]["pipeline"]["route"]["router_role"], "router");
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["branches"],
        json!([{ "target_role": "trend_trader" }, { "target_role": "range_trader" }]),
        "setup must persist canonical branch target roles, not raw stdin spelling",
    );
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["context_fields"],
        json!([
            "market_snapshot",
            "tool_state",
            "available_targets",
            "regime_summary"
        ]),
        "explicit empty context_fields must persist as the engine/API default router context",
    );

    let show = assert_json_success(&xvn(&["strategy", "show", &strategy_id], dir.path()));
    assert_eq!(
        show["pipeline"]["route"], body["strategy"]["pipeline"]["route"],
        "setup must write the normalized route to the saved strategy JSON",
    );
}

#[test]
fn strategy_route_setup_wizard_persists_normalized_route() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(
        dir.path(),
        "route-setup-wizard",
        &route_builder_agents_with_regime_filter(),
        None,
    );

    let out = xvn_with_stdin(
        &[
            "strategy",
            "route",
            "setup",
            &strategy_id,
            "--json",
        ],
        dir.path(),
        "router\ntrend_trader,range_trader\nrouter\ntrend_trader\nregime=trend\nmarket_snapshot,tool_state,available_targets\ncompact\ny\n",
    );

    let body = assert_json_success(&out);
    assert_eq!(body["strategy"]["manifest"]["id"], strategy_id);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(
        body["readiness"]["context_fields"],
        json!(["market_snapshot", "tool_state", "available_targets"]),
        "wizard setup must return the same readiness envelope as JSON/stdin setup",
    );
    assert_eq!(body["strategy"]["pipeline"]["route"]["router_role"], "router");
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["branches"],
        json!([{ "target_role": "trend_trader" }, { "target_role": "range_trader" }]),
        "wizard branch target answers must persist as normalized route branches",
    );
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["graph_edges"],
        json!([{
            "from_role": "router",
            "to_role": "trend_trader",
            "condition": { "eq": { "signal_field": "regime", "value": "trend" } }
        }]),
        "wizard graph source must be the selected router so the route graph is backend-valid under the guided contract",
    );
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["context_fields"],
        json!(["market_snapshot", "tool_state", "available_targets"]),
    );
    assert_eq!(body["strategy"]["pipeline"]["route"]["trace_mode"], "compact");

    let show = assert_json_success(&xvn(&["strategy", "show", &strategy_id], dir.path()));
    assert_eq!(
        show["pipeline"]["route"], body["strategy"]["pipeline"]["route"],
        "wizard setup must save the same normalized route returned in the JSON envelope",
    );
}

#[test]
fn strategy_route_setup_dry_run_reports_candidate_without_persisting() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(dir.path(), "route-setup-dry-run", &route_builder_agents(), None);
    let route = json!({
        "router_role": "router",
        "branches": [{ "target_role": "trend_trader" }],
        "context_fields": ["available_targets"],
        "trace_mode": "compact"
    })
    .to_string();

    let out = xvn_with_stdin(
        &[
            "strategy",
            "route",
            "setup",
            &strategy_id,
            "--from-stdin",
            "--dry-run",
            "--json",
        ],
        dir.path(),
        &route,
    );

    let body = assert_json_success(&out);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(
        body["readiness"]["context_fields"],
        json!(["available_targets"]),
        "dry-run readiness must be calculated from the supplied candidate route",
    );
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["branches"],
        json!([{ "target_role": "trend_trader" }]),
        "dry-run output should preview the candidate route operators supplied",
    );

    let reloaded = load_strategy(dir.path(), &strategy_id);
    assert!(
        reloaded.pipeline.route.is_none(),
        "setup --dry-run must not persist the candidate route",
    );
}

#[test]
fn strategy_route_validate_current_route_reports_readiness() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(
        dir.path(),
        "route-validate-current",
        &route_builder_agents(),
        Some(current_route()),
    );

    let out = xvn(
        &["strategy", "route", "validate", &strategy_id, "--json"],
        dir.path(),
    );

    let body = assert_json_success(&out);
    assert_eq!(body["strategy"]["manifest"]["id"], strategy_id);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(
        body["readiness"]["context_fields"],
        json!(["market_snapshot", "available_targets"]),
        "validate must report readiness for the currently saved route when no stdin/file override is supplied",
    );
}

#[test]
fn strategy_route_validate_from_stdin_reports_candidate_without_persisting() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(dir.path(), "route-validate-stdin", &route_builder_agents(), None);
    let candidate = json!({
        "router_role": "router",
        "branches": [{ "target_role": "range_trader" }],
        "context_fields": ["market_snapshot", "tool_state"],
        "trace_mode": "full"
    })
    .to_string();

    let out = xvn_with_stdin(
        &[
            "strategy",
            "route",
            "validate",
            &strategy_id,
            "--from-stdin",
            "--json",
        ],
        dir.path(),
        &candidate,
    );

    let body = assert_json_success(&out);
    assert_eq!(body["readiness"]["routed"], true);
    assert_eq!(
        body["strategy"]["pipeline"]["route"]["trace_mode"], "full",
        "validate --from-stdin should preview the supplied candidate route",
    );
    assert_eq!(
        body["readiness"]["context_fields"],
        json!(["market_snapshot", "tool_state"]),
    );

    let reloaded = load_strategy(dir.path(), &strategy_id);
    assert!(
        reloaded.pipeline.route.is_none(),
        "route validate --from-stdin is a dry-run and must not persist the candidate route",
    );
}

#[test]
fn strategy_route_setup_from_missing_json_path_emits_json_error_under_json_flag() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(
        dir.path(),
        "route-setup-missing-json",
        &route_builder_agents(),
        None,
    );
    let missing_path = dir.path().join("missing-route.json");

    let out = xvn(
        &[
            "strategy",
            "route",
            "setup",
            &strategy_id,
            "--from-json",
            missing_path.to_str().unwrap(),
            "--json",
        ],
        dir.path(),
    );

    assert_json_route_error(&out, &strategy_id, "read route JSON");
}

#[test]
fn strategy_route_setup_json_errors_are_diagnostic_and_do_not_partially_persist() {
    struct Case {
        name: &'static str,
        agents: Vec<(&'static str, Capability)>,
        stdin: String,
        expected_message: &'static str,
    }

    let cases = vec![
        Case {
            name: "empty stdin",
            agents: route_builder_agents(),
            stdin: String::new(),
            expected_message: "stdin is empty",
        },
        Case {
            name: "invalid JSON",
            agents: route_builder_agents(),
            stdin: "{".into(),
            expected_message: "invalid route JSON",
        },
        Case {
            name: "unknown context field",
            agents: route_builder_agents(),
            stdin: json!({
                "router_role": "router",
                "branches": [{ "target_role": "trend_trader" }],
                "context_fields": ["market_snapshot", "operator_notes"],
                "trace_mode": "compact"
            })
            .to_string(),
            expected_message: "operator_notes",
        },
        Case {
            name: "duplicate target",
            agents: route_builder_agents(),
            stdin: json!({
                "router_role": "router",
                "branches": [{ "target_role": "Trend_Trader" }, { "target_role": " trend_trader " }],
                "context_fields": ["available_targets"],
                "trace_mode": "compact"
            })
            .to_string(),
            expected_message: "route contract contains duplicate branch target 'trend_trader'",
        },
        Case {
            name: "self target",
            agents: route_builder_agents(),
            stdin: json!({
                "router_role": "router",
                "branches": [{ "target_role": "router" }],
                "context_fields": ["available_targets"],
                "trace_mode": "compact"
            })
            .to_string(),
            expected_message: "route contract branch target 'router' cannot point at the router itself",
        },
        Case {
            name: "backward target",
            agents: vec![
                ("trend_trader", Capability::Trader),
                ("router", Capability::Router),
                ("range_trader", Capability::Trader),
            ],
            stdin: json!({
                "router_role": "router",
                "branches": [{ "target_role": "trend_trader" }],
                "context_fields": ["available_targets"],
                "trace_mode": "compact"
            })
            .to_string(),
            expected_message: "route contract branch target 'trend_trader' must appear after router 'router' unless an explicit path mapping is present",
        },
        Case {
            name: "no trader target",
            agents: vec![("router", Capability::Router), ("analyst", Capability::Filter)],
            stdin: json!({
                "router_role": "router",
                "branches": [{ "target_role": "analyst" }],
                "context_fields": ["available_targets"],
                "trace_mode": "compact"
            })
            .to_string(),
            expected_message: "route contract branches must reach at least one Trader-capable decision path",
        },
    ];

    for case in cases {
        let dir = tempdir().unwrap();
        let strategy_id = seed_route_strategy(dir.path(), case.name, &case.agents, None);

        let out = xvn_with_stdin(
            &[
                "strategy",
                "route",
                "setup",
                &strategy_id,
                "--from-stdin",
                "--json",
            ],
            dir.path(),
            &case.stdin,
        );

        assert_json_route_error(&out, &strategy_id, case.expected_message);
        let reloaded = load_strategy(dir.path(), &strategy_id);
        assert!(
            reloaded.pipeline.route.is_none(),
            "invalid route setup case `{}` must not leave partial route state on disk",
            case.name,
        );
    }
}

#[test]
fn strategy_route_validate_json_errors_use_route_diagnostic_shape_without_raw_runtime_names() {
    let dir = tempdir().unwrap();
    let strategy_id = seed_route_strategy(
        dir.path(),
        "route-validate-diagnostics",
        &[
            ("router", Capability::Router),
            ("backup_router", Capability::Router),
        ],
        None,
    );
    let invalid_route = json!({
        "router_role": "router",
        "branches": [{ "target_role": "backup_router" }],
        "graph_edges": [{
            "from_role": "backup_router",
            "to_role": "router"
        }],
        "context_fields": ["available_targets"],
        "trace_mode": "compact"
    })
    .to_string();

    let out = xvn_with_stdin(
        &[
            "strategy",
            "route",
            "validate",
            &strategy_id,
            "--from-stdin",
            "--json",
        ],
        dir.path(),
        &invalid_route,
    );

    assert_ne!(
        code(&out),
        0,
        "non-launchable route validation should be machine-gateable; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "--json route validation failures must be JSON; stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert_eq!(body["strategy_id"], strategy_id);
    assert_eq!(
        body["readiness"]["launchable"], false,
        "CLI JSON must expose route launch gating without callers parsing prose; body={body:#}"
    );
    let reasons = body["readiness"]["reasons"].as_array().unwrap_or_else(|| {
        panic!("CLI JSON route diagnostics must include readiness.reasons; body={body:#}")
    });
    assert!(
        reasons
            .iter()
            .any(|reason| reason["code"] == "route.no_decision_path"),
        "CLI JSON must carry the no-decision route diagnostic code; reasons={reasons:#?}",
    );
    assert!(
        reasons
            .iter()
            .any(|reason| reason["code"] == "route.unsupported_graph"),
        "CLI JSON must carry the unsupported graph route diagnostic code; reasons={reasons:#?}",
    );
    let reason_text = serde_json::to_string(reasons).unwrap();
    for forbidden in ["target_agent_ref_index", "agent_ref_index", "PipelineKind"] {
        assert!(
            !reason_text.contains(forbidden),
            "operator-facing CLI diagnostics must use Route Builder copy, not `{forbidden}`; reasons={reason_text}",
        );
    }

    let reloaded = load_strategy(dir.path(), &strategy_id);
    assert!(
        reloaded.pipeline.route.is_none(),
        "route validate diagnostics must remain a dry-run and not persist invalid route state",
    );
}
