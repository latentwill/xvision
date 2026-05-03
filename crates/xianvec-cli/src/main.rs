//! xvn — XIANVEC CLI entry point.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "xvn",
    version,
    about = "XIANVEC: vectors-on vs vectors-off trading agent"
)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _ = Cli::parse();
    println!("xvn v0.1.0 — see `xvn --help`");
    Ok(())
}
