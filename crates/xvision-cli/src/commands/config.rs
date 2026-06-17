//! `xvn config get <key>` / `xvn config set <key> <value>`.
//!
//! Reads and writes dot-namespaced operator config keys persisted in the
//! `xvn_config` SQLite table (migration 069). Currently supports:
//!   autoresearch.min_precision_lift_pp
//!   autoresearch.max_pnl_regression
//!   autoresearch.promotion_epsilon
//!   autoresearch.promotion_acc_floor
//!   autoresearch.promotion_min_holdout
//!   autoresearch.min_cycle_count
//!   autoresearch.train_wall_clock_sec
//!   autoresearch.price_forward_threshold
//!
//! Future config namespaces (e.g. `optimizer.*`) extend this same verb.
//! Do NOT add new top-level verbs for per-namespace config.

use clap::{Args, Subcommand};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::nanochat::config_store::{
    get_config, set_config, DEFAULT_MAX_PNL_REGRESSION, DEFAULT_MIN_CYCLE_COUNT,
    DEFAULT_MIN_PRECISION_LIFT_PP, DEFAULT_PRICE_FORWARD_THRESHOLD, DEFAULT_PROMOTION_ACC_FLOOR,
    DEFAULT_PROMOTION_EPSILON, DEFAULT_PROMOTION_MIN_HOLDOUT, DEFAULT_TRAIN_WALL_CLOCK_SEC,
};

use crate::commands::home::resolve_xvn_home;

#[derive(Args, Debug)]
pub struct ConfigCmd {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Print the current value (or default) for a config key.
    ///
    /// Example:
    ///   xvn config get autoresearch.promotion_epsilon
    Get {
        /// Dotted config key, e.g. autoresearch.promotion_epsilon
        key: String,
        #[arg(long)]
        xvn_home: Option<std::path::PathBuf>,
    },
    /// Set a config key, validating the value before writing.
    ///
    /// Examples:
    ///   xvn config set autoresearch.promotion_epsilon 0.02
    ///   xvn config set autoresearch.promotion_min_holdout 300
    Set {
        /// Dotted config key, e.g. autoresearch.promotion_epsilon
        key: String,
        /// Value (validated per key-specific numeric range rules).
        value: String,
        #[arg(long)]
        xvn_home: Option<std::path::PathBuf>,
    },
}

pub async fn run(cmd: ConfigCmd) -> anyhow::Result<()> {
    match cmd.action {
        ConfigAction::Get { key, xvn_home } => {
            let home = resolve_xvn_home(xvn_home)?;
            let ctx = ApiContext::open(
                &home,
                Actor::Cli {
                    user: "operator".into(),
                },
            )
            .await?;
            let stored = get_config(&ctx.db, &key).await?;
            let display = match stored.as_deref() {
                Some(v) => format!("{key} = {v}"),
                None => {
                    let default = config_default_display(&key);
                    format!("{key} = {default} (default)")
                }
            };
            println!("{display}");
            Ok(())
        }
        ConfigAction::Set { key, value, xvn_home } => {
            let home = resolve_xvn_home(xvn_home)?;
            let ctx = ApiContext::open(
                &home,
                Actor::Cli {
                    user: "operator".into(),
                },
            )
            .await?;
            set_config(&ctx.db, &key, &value).await?;
            println!("ok: {key} = {value}");
            Ok(())
        }
    }
}

fn config_default_display(key: &str) -> String {
    match key {
        "autoresearch.min_precision_lift_pp" => DEFAULT_MIN_PRECISION_LIFT_PP.to_string(),
        "autoresearch.max_pnl_regression" => DEFAULT_MAX_PNL_REGRESSION.to_string(),
        "autoresearch.promotion_epsilon" => DEFAULT_PROMOTION_EPSILON.to_string(),
        "autoresearch.promotion_acc_floor" => DEFAULT_PROMOTION_ACC_FLOOR.to_string(),
        "autoresearch.promotion_min_holdout" => DEFAULT_PROMOTION_MIN_HOLDOUT.to_string(),
        "autoresearch.min_cycle_count" => DEFAULT_MIN_CYCLE_COUNT.to_string(),
        "autoresearch.train_wall_clock_sec" => DEFAULT_TRAIN_WALL_CLOCK_SEC.to_string(),
        "autoresearch.price_forward_threshold" => DEFAULT_PRICE_FORWARD_THRESHOLD.to_string(),
        _ => "(unknown key — run `xvn config get` with a valid autoresearch.* key)".into(),
    }
}
