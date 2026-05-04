use std::path::PathBuf;

use uuid::Uuid;

pub async fn run(_setup_id: Uuid, _db: PathBuf) -> anyhow::Result<()> {
    println!("show-decision: not yet wired");
    Ok(())
}
