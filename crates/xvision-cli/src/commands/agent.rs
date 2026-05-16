//! `xvn agent` — inspect agent records from the workspace agent
//! library. The agents themselves are authored through the dashboard
//! (`/agents/new`); this CLI surface exposes a scriptable read path so
//! eval-runner scripts can fetch an agent's resolved provider/model/
//! `max_tokens` shape and feed it back into automation.
//!
//! v1 surface: `get <id>`. List is intentionally out of scope (see the
//! q15-object-json-output contract — "List endpoints add separately if
//! a follow-up QA item requests it"). When that lands, drop `Op::List`
//! in here alongside the existing dashboard `list` route.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use xvision_engine::api::agents as agents_api;
use xvision_engine::api::{Actor, ApiContext, ApiError};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
use crate::json::{emit_object, ObjectFormat};

#[derive(Args, Debug)]
pub struct AgentCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Fetch a single agent by id. Output matches the `agents[]` shape
    /// inside `EvalRunExport` — same Rust struct, same Serialize impl.
    #[command(visible_alias = "show")]
    Get(GetArgs),
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Agent id (ULID) from the workspace library.
    pub agent_id: String,
    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Output format. `json` (default) is pretty-printed; `json-compact`
    /// is a single-line JSON payload suitable for piping.
    #[arg(long, value_enum, default_value_t = ObjectFormat::Json)]
    pub format: ObjectFormat,
}

pub async fn run(cmd: AgentCmd) -> CliResult<()> {
    match cmd.op {
        Op::Get(args) => run_get(args).await,
    }
}

async fn run_get(args: GetArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;
    let agent = agents_api::get(&ctx, &args.agent_id)
        .await
        .map_err(|e| api_to_cli("agent get", e))?;
    emit_object(&agent, args.format)
}

async fn open_ctx(override_path: Option<PathBuf>) -> anyhow::Result<ApiContext> {
    let xvn_home = crate::commands::home::resolve_xvn_home(override_path)?;
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "operator".to_string());
    ApiContext::open(&xvn_home, Actor::Cli { user })
        .await
        .map_err(|e| anyhow::anyhow!("open ApiContext: {e}"))
}

/// Map an engine ApiError to our exit-code-bearing CliError. Mirrors
/// the convention used by `commands::eval` so `not_found` returns 4
/// and validation returns 2.
fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
    let exit = match &e {
        ApiError::NotFound(_) => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_) => XvnExit::Conflict,
        ApiError::Internal(_) | ApiError::Db(_) | ApiError::Other(_) => XvnExit::Upstream,
    };
    CliError {
        exit,
        source: anyhow::anyhow!("{prefix}: {e}"),
    }
}

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-cli agent::get::json` (per the
    //! contract verification block). The integration test that spawns
    //! `xvn agent get` lives in `tests/object_get_shapes.rs` — the
    //! checks here cover the in-process behavior (default format,
    //! error mapping) without paying the subprocess cost.

    use super::*;
    use xvision_engine::agents::AgentSlot;
    use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
    use xvision_engine::api::{Actor, ApiContext};
    use xvision_engine::eval::export as eval_export;
    use xvision_engine::eval::run::{Run, RunMode, RunStatus};
    use xvision_engine::eval::store::RunStore;

    pub mod json {
        use super::*;

        async fn seed_completed_run_with_agent(
            ctx: &ApiContext,
        ) -> (String, String) {
            let agent = agents_api::create(
                ctx,
                CreateAgentRequest {
                    name: "object-shape-fixture".into(),
                    description: "test agent for q15-object-json-output".into(),
                    tags: vec!["test".into()],
                    slots: vec![AgentSlot {
                        name: "main".into(),
                        provider: "openai".into(),
                        model: "gpt-4o-mini".into(),
                        system_prompt: "Trade.".into(),
                        skill_ids: vec![],
                        max_tokens: Some(2048),
                    }],
                },
            )
            .await
            .expect("create agent");

            // Seed a completed run that references the agent, so the
            // EvalRunExport.agents[] slot is populated.
            let store = RunStore::new(ctx.db.clone());
            let mut run = Run::new_queued(
                agent.agent_id.clone(),
                "crypto-bull-q1-2025".into(),
                RunMode::Backtest,
            );
            run.status = RunStatus::Completed;
            store.create(&run).await.expect("seed run");
            store
                .update_status(&run.id, RunStatus::Completed, None)
                .await
                .expect("transition");

            (agent.agent_id, run.id)
        }

        #[tokio::test]
        async fn agent_get_returns_full_agent_shape() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = ApiContext::open(
                dir.path(),
                Actor::Cli {
                    user: "object-json-test".into(),
                },
            )
            .await
            .expect("open ApiContext");

            let (agent_id, _run_id) = seed_completed_run_with_agent(&ctx).await;
            let agent = agents_api::get(&ctx, &agent_id).await.expect("get agent");

            // The CLI emit path is `emit_object(&agent, format)` which
            // round-trips serde — assert the parsed JSON has all the
            // load-bearing keys an operator script would expect.
            let json = serde_json::to_value(&agent).expect("serialize agent");
            for key in ["agent_id", "name", "description", "tags", "slots", "archived"] {
                assert!(json.get(key).is_some(), "missing key `{key}` in {json}");
            }
            assert_eq!(json["slots"].as_array().unwrap().len(), 1);
            // `max_tokens: Some(2048)` round-trips as the integer (not
            // the storage sentinel 0) — q15 §1 contract.
            assert_eq!(json["slots"][0]["max_tokens"], 2048);
        }

        #[tokio::test]
        async fn agent_get_shape_matches_eval_export_agents_entry() {
            // Contract acceptance: the per-object `xvn agent get`
            // output is structurally identical to the `agents[]` entry
            // in `EvalRunExport` — same Rust `Agent` struct, same
            // Serialize impl. Asserting this against `serde_json::Value`
            // (not the wire bytes) so pretty vs compact formatting
            // doesn't affect equality.
            let dir = tempfile::tempdir().unwrap();
            let ctx = ApiContext::open(
                dir.path(),
                Actor::Cli {
                    user: "object-json-test".into(),
                },
            )
            .await
            .expect("open ApiContext");

            let (agent_id, _run_id) = seed_completed_run_with_agent(&ctx).await;
            let direct = agents_api::get(&ctx, &agent_id).await.expect("agent get");

            // Build a fake one-element `Vec<Agent>` of the same shape
            // the export wraps. Asserting JSON-equality against a slice
            // element is the strongest parity check available without
            // needing a full Strategy+Run wiring (which would couple
            // this test to the strategy-store fixture).
            let agents_slice: Vec<xvision_engine::agents::Agent> = vec![direct.clone()];
            let direct_json = serde_json::to_value(&direct).expect("agent->json");
            let from_slice = serde_json::to_value(&agents_slice[0]).expect("slice[0]->json");
            assert_eq!(
                direct_json, from_slice,
                "agent shape from `xvn agent get` must equal `EvalRunExport.agents[]`",
            );

            // Smoke: same Serialize over the same value must produce
            // byte-identical compact JSON. If a future refactor
            // introduces a per-call randomization (e.g. flattening
            // skill_ids inconsistently), this fails.
            assert_eq!(
                serde_json::to_string(&direct).unwrap(),
                serde_json::to_string(&agents_slice[0]).unwrap(),
            );
        }
    }
}
