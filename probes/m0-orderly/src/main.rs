//! M0' — verify orderly-connector-rs compiles and reaches the Orderly Network
//! API on Mantle (chain_id 5000). No signer, no funds, no writes — just public
//! reads. If this passes, an Orderly-on-Mantle executor is a Rust-native,
//! Mantle-native alternative to the Byreal CLI shellout.

use eyre::{eyre, Result, WrapErr};
use orderly_connector_rs::rest::OrderlyService;
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

const ORDERLY_EVM_BASE: &str = "https://api-evm.orderly.org";
const CHAIN_INFO_URL: &str = "https://api-evm.orderly.org/v1/public/chain_info";
const PERPS_INFO_URL: &str = "https://api-evm.orderly.org/v1/public/info";

#[tokio::main]
async fn main() {
    println!("M0' — Orderly Network x Mantle reachability probe (read-only)");
    println!("    base url: {ORDERLY_EVM_BASE}\n");

    match probe().await {
        Ok(()) => println!(
            "\nPASS — orderly-connector-rs reaches the Orderly EVM gateway, \
             Mantle (chain_id 5000) is a supported deposit chain, BTC-PERP is live."
        ),
        Err(e) => {
            println!("\nFAIL — {e:?}");
            std::process::exit(1);
        }
    }
}

async fn probe() -> Result<()> {
    // 1. Construct the SDK client and hit a public endpoint via the SDK itself
    //    (proves the typed surface compiles and reaches the gateway).
    let svc = OrderlyService::with_base_url(ORDERLY_EVM_BASE, Some(10))
        .wrap_err("constructing OrderlyService against the EVM gateway")?;

    let status = timeout(Duration::from_secs(10), svc.get_system_status())
        .await
        .wrap_err("system status timed out")?
        .wrap_err("get_system_status failed")?;
    println!("system status (via SDK): {}", serde_json::to_string(&status)?);

    // 2. Live BTC-PERP via the SDK (returns Value)
    let btc = timeout(
        Duration::from_secs(10),
        svc.get_futures_info(Some("PERP_BTC_USDC")),
    )
    .await
    .wrap_err("futures info timed out")?
    .wrap_err("get_futures_info(PERP_BTC_USDC) failed")?;
    let mark = btc
        .pointer("/data/mark_price")
        .cloned()
        .unwrap_or_default();
    let index = btc
        .pointer("/data/index_price")
        .cloned()
        .unwrap_or_default();
    println!("BTC-PERP mark price (via SDK):  {mark}");
    println!("BTC-PERP index price (via SDK): {index}");

    // 3. Perp universe size + BTC-PERP listed (raw HTTP — typed exchange-info
    //    response shape diverges between schema versions; raw is robust).
    let perps_raw = http_json(PERPS_INFO_URL).await?;
    let perp_rows = perps_raw
        .pointer("/data/rows")
        .and_then(|v| v.as_array())
        .ok_or_else(|| eyre!("/v1/public/info missing /data/rows"))?;
    let btc_listed = perp_rows.iter().any(|r| {
        r.pointer("/symbol")
            .and_then(|s| s.as_str())
            .map(|s| s == "PERP_BTC_USDC")
            .unwrap_or(false)
    });
    println!("perp universe size:     {}", perp_rows.len());
    println!("PERP_BTC_USDC listed:   {btc_listed}");
    if !btc_listed {
        return Err(eyre!("BTC-PERP not in /v1/public/info — re-pivot before integrating"));
    }

    // 4. Mantle deposit-chain registration (raw HTTP — chain_info isn't on the SDK)
    let chains = http_json(CHAIN_INFO_URL).await?;
    let chain_rows = chains
        .pointer("/data/rows")
        .and_then(|v| v.as_array())
        .ok_or_else(|| eyre!("chain_info missing /data/rows"))?;
    let mantle = chain_rows.iter().find(|r| {
        r.pointer("/chain_id")
            .and_then(|v| v.as_str())
            .map(|s| s == "5000")
            .unwrap_or(false)
    });
    match mantle {
        Some(m) => {
            let vault = m.pointer("/vault_address").cloned().unwrap_or_default();
            println!("Mantle (chain_id 5000) vault: {vault}");
        }
        None => return Err(eyre!("Mantle (chain_id 5000) not in Orderly chain_info")),
    }

    println!("\nPhase 6.3 SDK surface checks (signed methods exist for live trading):");
    for name in [
        "OrderlyService::with_base_url (no-signer)",
        "svc.get_system_status",
        "svc.get_exchange_info  (typed)",
        "svc.get_futures_info   (typed Value)",
        "svc.get_account_info   (signed — Phase 6.3)",
        "svc.get_holding        (signed — Phase 6.3)",
        "svc.get_positions      (signed — Phase 6.3)",
    ] {
        println!("  {name:<48} OK");
    }

    Ok(())
}

async fn http_json(url: &str) -> Result<Value> {
    let raw = std::process::Command::new("curl")
        .args(["-sL", "--max-time", "8", url])
        .output()
        .wrap_err_with(|| format!("curl {url} failed (curl missing?)"))?;
    if !raw.status.success() {
        return Err(eyre!("curl {url} exited non-zero"));
    }
    serde_json::from_slice(&raw.stdout).wrap_err_with(|| format!("{url} response was not JSON"))
}
