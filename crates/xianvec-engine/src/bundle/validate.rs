use thiserror::Error;

use crate::bundle::StrategyBundle;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("strategy must have at least one filled LLM slot")]
    NoLlmSlots,
    #[error("asset universe cannot be empty")]
    EmptyAssetUniverse,
    #[error("invalid risk config: {0}")]
    InvalidRisk(String),
    #[error("required tool '{0}' not in any slot's allowed_tools")]
    UndeclaredTool(String),
}

pub fn validate_bundle(b: &StrategyBundle) -> Result<(), ValidationError> {
    if b.regime_slot.is_none() && b.intern_slot.is_none() && b.trader_slot.is_none() {
        return Err(ValidationError::NoLlmSlots);
    }
    if b.manifest.asset_universe.is_empty() {
        return Err(ValidationError::EmptyAssetUniverse);
    }
    if b.risk.risk_pct_per_trade <= 0.0 || b.risk.risk_pct_per_trade > 0.5 {
        return Err(ValidationError::InvalidRisk(format!(
            "risk_pct_per_trade must be in (0, 0.5], got {}",
            b.risk.risk_pct_per_trade
        )));
    }
    if b.risk.max_leverage <= 0.0 || b.risk.max_leverage > 100.0 {
        return Err(ValidationError::InvalidRisk(format!(
            "max_leverage must be in (0, 100], got {}",
            b.risk.max_leverage
        )));
    }
    Ok(())
}
