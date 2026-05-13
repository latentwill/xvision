use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use xvision_engine::templates::registry;

#[derive(Args, Debug)]
pub struct DoctorCmd {
    /// Override the xvn home directory (default: $XVN_HOME or ~/.xvn).
    #[arg(long)]
    pub xvn_home: Option<PathBuf>,
    /// Emit a machine-readable report.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    xvn_home: String,
    db_path: String,
    config_path: String,
    provider_secrets_path: String,
    broker_secrets_path: String,
    strategies_dir: String,
    templates: Vec<String>,
    config_exists: bool,
    provider_secrets_exists: bool,
    broker_secrets_exists: bool,
    remote_target: String,
}

pub async fn run(cmd: DoctorCmd) -> anyhow::Result<()> {
    let xvn_home = crate::commands::home::resolve_xvn_home(cmd.xvn_home)?;
    let config_path = runtime_config_path(&xvn_home);
    let provider_secrets_path = xvn_home.join("secrets").join("providers.toml");
    let broker_secrets_path = xvn_home.join("secrets").join("brokers.toml");
    let report = DoctorReport {
        xvn_home: xvn_home.display().to_string(),
        db_path: xvn_home.join("xvn.db").display().to_string(),
        config_path: config_path.display().to_string(),
        provider_secrets_path: provider_secrets_path.display().to_string(),
        broker_secrets_path: broker_secrets_path.display().to_string(),
        strategies_dir: xvn_home.join("strategies").display().to_string(),
        templates: registry::list_template_names(),
        config_exists: config_path.exists(),
        provider_secrets_exists: provider_secrets_path.exists(),
        broker_secrets_exists: broker_secrets_path.exists(),
        remote_target: std::env::var("XVN_REMOTE_URL").unwrap_or_else(|_| "local".to_string()),
    };

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("xvn_home              {}", report.xvn_home);
        println!("db_path               {}", report.db_path);
        println!("config_path           {}", report.config_path);
        println!("provider_secrets      {}", report.provider_secrets_path);
        println!("broker_secrets        {}", report.broker_secrets_path);
        println!("strategies_dir        {}", report.strategies_dir);
        println!("remote_target         {}", report.remote_target);
        println!("config_exists         {}", report.config_exists);
        println!("provider_secrets      {}", report.provider_secrets_exists);
        println!("broker_secrets        {}", report.broker_secrets_exists);
        println!("templates             {}", report.templates.join(", "));
    }

    Ok(())
}

fn runtime_config_path(xvn_home: &std::path::Path) -> PathBuf {
    if let Ok(path) = std::env::var("XVN_CONFIG_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    xvn_home.join("config").join("default.toml")
}
