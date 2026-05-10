//! `xvn show-briefing` — read a cached `InternBriefing` from the SQLite store.

use std::path::PathBuf;

use uuid::Uuid;
use xianvec_core::store::Store;

pub async fn run(cycle_id: Uuid, db: PathBuf) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", db.display());
    let store = Store::open(&url).await?;
    match store.get_briefing(&cycle_id).await? {
        Some(b) => {
            println!("{}", serde_json::to_string_pretty(&b)?);
        }
        None => {
            println!("no briefing found for cycle_id={cycle_id}");
        }
    }
    Ok(())
}
