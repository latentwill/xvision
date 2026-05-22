//! `xvn show-decision` — pretty-print cached `TraderDecision`(s) for a setup.

use std::path::PathBuf;

use uuid::Uuid;
use xvision_core::store::Store;

pub async fn run(cycle_id: Uuid, db: PathBuf) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", db.display());
    let store = Store::open(&url).await?;
    let decisions = store.get_decisions_for_setup(&cycle_id).await?;

    if decisions.is_empty() {
        println!("no decisions found for cycle_id={cycle_id}");
        return Ok(());
    }
    println!(
        "XVISION decisions for cycle_id={cycle_id} ({} arm(s)):",
        decisions.len()
    );
    for (arm, decision) in decisions {
        println!();
        println!("--- arm: {arm} ---");
        println!("{}", serde_json::to_string_pretty(&decision)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use xvision_core::trading::{Action, AssetSymbol, Direction, TraderDecision};

    #[tokio::test]
    async fn show_decision_round_trips_an_inserted_row() {
        let store = Store::open("sqlite://:memory:")
            .await
            .expect("open in-memory store");

        let cycle_id = Uuid::new_v4();
        let decision = TraderDecision {
            cycle_id,
            action: Action::Buy,
            size_bps: 800,
            direction: Direction::Long,
            stop_loss_pct: 2.0,
            take_profit_pct: 5.0,
            trader_summary: "show-decision smoke fixture decision.".into(),
            asset: AssetSymbol::Btc,
        };
        store
            .upsert_cycle(&cycle_id, AssetSymbol::Btc.as_str(), 24, &serde_json::json!({}))
            .await
            .unwrap();
        store.insert_decision("trader_arm", &decision).await.unwrap();

        // Hit the read path directly (run() takes a PathBuf, but in-memory
        // sqlite needs the string form — we exercise the same fetch logic).
        let fetched = store.get_decisions_for_setup(&cycle_id).await.unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].0, "trader_arm");
        assert_eq!(fetched[0].1.action, Action::Buy);
    }
}
