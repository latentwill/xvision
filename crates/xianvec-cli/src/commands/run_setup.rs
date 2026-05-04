use std::path::PathBuf;

pub async fn run(_snapshot: PathBuf, _intern: String, _model: String) -> anyhow::Result<()> {
    println!("run-setup: not yet wired");
    Ok(())
}
