//! `xvn dashboard serve` — boot the embedded SPA + axum API on localhost.

use std::net::SocketAddr;

use anyhow::Context;
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct DashboardCmd {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Start the dashboard HTTP server (embedded SPA + /api/*).
    Serve(ServeArgs),
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Bind address. Defaults to localhost:8788.
    #[arg(long, default_value = "127.0.0.1:8788")]
    pub bind: String,
}

pub async fn run(cmd: DashboardCmd) -> anyhow::Result<()> {
    match cmd.op {
        Op::Serve(args) => {
            let addr: SocketAddr = args
                .bind
                .parse()
                .with_context(|| format!("invalid --bind address: {}", args.bind))?;
            xvision_dashboard::serve(addr).await
        }
    }
}
