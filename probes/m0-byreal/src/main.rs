//! M0 — pre-skeleton verification that xvision's Rust runtime can drive
//! `@byreal-io/byreal-perps-cli` via `tokio::process::Command` and parse its
//! JSON output. This is the exact integration shape Phase 6.3 will use, just
//! reduced to a read-only `catalog` call (no auth, no funds, no writes).
//!
//! The hackathon's Path 1 ("DeFi Deep Dive") explicitly names Byreal Perps
//! CLI / Byreal Agent Skills / RealClaw as winning tooling. Trades execute on
//! Hyperliquid; identity/reputation lives on Mantle (ERC-8004) — Phase 6.5.

use eyre::{eyre, Result, WrapErr};
use serde::Deserialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Deserialize)]
struct Envelope {
    success: bool,
    meta: Meta,
    data: CatalogData,
}

#[derive(Debug, Deserialize)]
struct Meta {
    version: String,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct CatalogData {
    capabilities: Vec<Capability>,
}

#[derive(Debug, Deserialize)]
struct Capability {
    id: String,
    name: String,
    category: String,
    auth_required: bool,
    command: String,
}

async fn run_catalog() -> Result<Envelope> {
    let mut child = Command::new("npx")
        .args(["-y", "@byreal-io/byreal-perps-cli@latest", "catalog", "-o", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .wrap_err("failed to spawn npx — is Node.js installed?")?;

    let output = timeout(Duration::from_secs(60), child.wait_with_output())
        .await
        .wrap_err("npx byreal-perps-cli timed out after 60s")?
        .wrap_err("npx process exited with error")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!(
            "byreal-perps-cli exited {}: {stderr}",
            output.status.code().unwrap_or(-1)
        ));
    }

    serde_json::from_slice::<Envelope>(&output.stdout)
        .wrap_err("byreal-perps-cli returned malformed JSON for `catalog -o json`")
}

#[tokio::main]
async fn main() {
    println!("M0 — Byreal Perps CLI subprocess + JSON contract probe");
    println!("    spawning: npx -y @byreal-io/byreal-perps-cli@latest catalog -o json\n");

    match run_catalog().await {
        Ok(env) => {
            if !env.success {
                println!("FAIL — CLI returned success=false at the envelope layer");
                std::process::exit(1);
            }
            println!("CLI version       : {}", env.meta.version);
            println!("response timestamp: {}", env.meta.timestamp);
            println!("capability count  : {}\n", env.data.capabilities.len());

            let queries: Vec<&Capability> = env
                .data
                .capabilities
                .iter()
                .filter(|c| c.category == "query")
                .collect();
            let executes: Vec<&Capability> = env
                .data
                .capabilities
                .iter()
                .filter(|c| c.category == "execute")
                .collect();

            println!("query commands  ({}) — usable without a signer:", queries.len());
            for c in &queries {
                let auth = if c.auth_required { " [auth]" } else { "" };
                println!("  {:<28} {}{}", c.id, c.command, auth);
            }
            println!(
                "\nexecute commands ({}) — wallet/private-key required:",
                executes.len()
            );
            for c in &executes {
                let auth = if c.auth_required { " [auth]" } else { "" };
                println!("  {:<28} {}{}", c.id, c.command, auth);
            }

            // Phase 6.3 hard-needs: order placement + position management
            // + signal queries. Confirm those primitives exist by id.
            let want = [
                "account.info",
                "order.market",
                "order.limit",
                "order.cancel",
                "position.list",
                "position.close",
                "signal.scan",
            ];
            let have: Vec<&str> = env
                .data
                .capabilities
                .iter()
                .map(|c| c.id.as_str())
                .collect();
            let missing: Vec<&&str> = want.iter().filter(|w| !have.contains(*w)).collect();

            println!("\nPhase 6.3 primitive coverage:");
            for w in &want {
                let mark = if have.contains(w) { "OK" } else { "MISSING" };
                println!("  {:<22} {}", w, mark);
            }

            if missing.is_empty() {
                println!("\nPASS — Byreal Perps CLI subprocess + JSON contract is the executor path.");
            } else {
                println!(
                    "\nPARTIAL — CLI works, but {} primitives are not exposed under expected ids:",
                    missing.len()
                );
                for m in &missing {
                    println!("    {m}");
                }
                println!("Expect to map these to whatever ids the catalog actually exposes during Phase 6.3.");
            }
        }
        Err(e) => {
            println!("FAIL — {e:?}");
            std::process::exit(1);
        }
    }
}
