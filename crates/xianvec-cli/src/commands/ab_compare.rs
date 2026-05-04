use std::path::PathBuf;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    _setups: PathBuf,
    _bars: PathBuf,
    _arms: String,
    _output: PathBuf,
    _initial_nav_usd: f64,
    _fee_bps: u32,
    _step_hours: u32,
    _horizon_hours: u32,
    _asset: String,
    _model: PathBuf,
    _tokenizer: PathBuf,
    _intern: String,
    _intern_model: String,
) -> anyhow::Result<()> {
    println!("ab-compare: not yet wired");
    Ok(())
}
