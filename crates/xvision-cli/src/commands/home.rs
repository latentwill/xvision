use std::path::PathBuf;

use anyhow::{Context, Result};

/// Resolve the effective XVN home with one precedence order for CLI commands:
/// explicit flag, then `XVN_HOME`, then `$HOME/.xvn`.
pub fn resolve_xvn_home(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    if let Ok(p) = std::env::var("XVN_HOME") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = dirs::home_dir().context("HOME not set; pass --xvn-home or set XVN_HOME")?;
    Ok(home.join(".xvn"))
}

pub fn resolve_xvn_home_env() -> Result<PathBuf> {
    resolve_xvn_home(None)
}
