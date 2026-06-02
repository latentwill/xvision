use std::path::PathBuf;

use xvision_engine::api::eval::{self, ListRunsRequest};
use xvision_engine::api::{Actor, ApiContext};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

pub async fn run(
    xvn_home: Option<PathBuf>,
    strategy: Option<String>,
    json: bool,
    n: usize,
) -> CliResult<()> {
    let ctx = open_ctx(xvn_home).await.exit_with(XvnExit::Upstream)?;

    let runs = eval::list(
        &ctx,
        ListRunsRequest {
            agent_id: strategy,
            scenario_id: None,
            status: None,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| CliError {
        exit: XvnExit::Upstream,
        source: anyhow::anyhow!("eval list: {e}"),
    })?;

    let recent: Vec<_> = runs.into_iter().take(n).collect();

    if recent.is_empty() {
        println!("(no runs)");
        return Ok(());
    }

    if json {
        crate::io::print_json(&recent)?;
        return Ok(());
    }

    for (i, run) in recent.iter().enumerate() {
        println!("run       {}", run.id);
        println!("status    {}", run.status.as_str());
        println!("strategy  {}", run.agent_id);
        println!("scenario  {}", run.scenario_id);
        println!("started   {}", run.started_at.format("%Y-%m-%d %H:%M UTC"));
        if let Some(c) = run.completed_at {
            println!("completed {}", c.format("%Y-%m-%d %H:%M UTC"));
        }
        if let Some(m) = run.metrics.as_ref() {
            println!();
            println!("return    {:.2}%", m.total_return_pct);
            println!("sharpe    {:.3}", m.sharpe);
            println!("drawdown  {:.2}%", m.max_drawdown_pct);
            println!("trades    {}", m.n_trades);
            println!("decisions {}", m.n_decisions);
        } else {
            println!();
            println!("(no metrics — run may still be in progress)");
        }
        if let Some(e) = run.error.as_deref() {
            println!("error     {e}");
        }
        if n > 1 && i + 1 < recent.len() {
            println!("---");
        }
    }

    Ok(())
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
