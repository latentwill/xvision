//! TOML loader for `config/risk.toml`.

use serde::Deserialize;
use std::path::Path;

use crate::RiskError;

#[derive(Debug, Clone, Deserialize)]
pub struct Limits {
    pub max_position_pct_nav: f64,
    pub max_total_exposure_pct: f64,
    pub max_open_positions: usize,
    pub max_daily_loss_pct: f64,
    pub max_correlation_cluster: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stops {
    pub stop_loss_required: bool,
    pub stop_loss_min_pct: f64,
    pub stop_loss_max_pct: f64,
    pub take_profit_required: bool,
    pub take_profit_min_rr: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub limits: Limits,
    pub stops: Stops,
}

impl RiskConfig {
    pub fn from_path(path: &Path) -> Result<Self, RiskError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| RiskError::Config(format!("cannot read {}: {e}", path.display())))?;
        let cfg: RiskConfig = toml::from_str(&raw)
            .map_err(|e| RiskError::Config(format!("parse error in {}: {e}", path.display())))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), RiskError> {
        let l = &self.limits;
        let s = &self.stops;

        if !(l.max_position_pct_nav > 0.0 && l.max_position_pct_nav < 100.0) {
            return Err(RiskError::Config(
                "max_position_pct_nav must be in (0, 100)".into(),
            ));
        }
        if !(l.max_total_exposure_pct > 0.0 && l.max_total_exposure_pct <= 500.0) {
            return Err(RiskError::Config(
                "max_total_exposure_pct must be in (0, 500]".into(),
            ));
        }
        if l.max_open_positions == 0 {
            return Err(RiskError::Config(
                "max_open_positions must be > 0".into(),
            ));
        }
        if !(l.max_daily_loss_pct > 0.0 && l.max_daily_loss_pct <= 100.0) {
            return Err(RiskError::Config(
                "max_daily_loss_pct must be in (0, 100]".into(),
            ));
        }
        if l.max_correlation_cluster == 0 {
            return Err(RiskError::Config(
                "max_correlation_cluster must be > 0".into(),
            ));
        }
        if s.stop_loss_min_pct <= 0.0 {
            return Err(RiskError::Config(
                "stop_loss_min_pct must be > 0".into(),
            ));
        }
        if s.stop_loss_max_pct <= s.stop_loss_min_pct {
            return Err(RiskError::Config(
                "stop_loss_max_pct must be > stop_loss_min_pct".into(),
            ));
        }
        if s.take_profit_min_rr <= 0.0 {
            return Err(RiskError::Config(
                "take_profit_min_rr must be > 0".into(),
            ));
        }
        Ok(())
    }
}
