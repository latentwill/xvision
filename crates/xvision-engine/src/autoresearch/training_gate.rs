//! Deploy-host guard for the local training loop.
//!
//! `XVN_ENABLE_LOCAL_TRAINING` must be explicitly set to `1` or `true`
//! for any training subprocess to be spawned. Deploy hosts (small VPS,
//! Coolify nodes) leave it unset, so `POST /api/autoresearch/runs` returns
//! 403 there. The route handler calls `require_training_enabled()` at the
//! top of the handler body — before any DB work — so the gate is enforced
//! regardless of internal state.

use anyhow::{bail, Result};

/// Returns `true` iff `XVN_ENABLE_LOCAL_TRAINING` is set to `1` or `true`
/// (case-insensitive). Any other value (including the absence of the var,
/// `0`, `false`) returns `false`.
pub fn training_enabled() -> bool {
    match std::env::var("XVN_ENABLE_LOCAL_TRAINING")
        .ok()
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("1") | Some("true") => true,
        _ => false,
    }
}

/// Returns `Ok(())` when training is enabled, else an `Err` that should be
/// surfaced as 403 Forbidden to the HTTP caller.
///
/// Use at the top of every handler that would spawn a training subprocess:
/// ```rust,ignore
/// require_training_enabled()?;
/// ```
pub fn require_training_enabled() -> Result<()> {
    if training_enabled() {
        Ok(())
    } else {
        bail!(
            "XVN_ENABLE_LOCAL_TRAINING is not set — local training is disabled \
             on this host. Set the env var to '1' on your local dev/build machine."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // XVN_ENABLE_LOCAL_TRAINING is process-global; cargo runs these tests in
    // parallel by default. Serialize every env-touching test through this lock
    // so one test's mutation can't be observed by another. The lock guard is
    // declared BEFORE the EnvGuard in each test so it drops AFTER it (Rust drops
    // in reverse declaration order) — the env is restored while the lock is held.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn gate_passes_when_env_set_to_1() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set("XVN_ENABLE_LOCAL_TRAINING", "1");
        assert!(training_enabled());
    }

    #[test]
    fn gate_passes_when_env_set_to_true() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set("XVN_ENABLE_LOCAL_TRAINING", "true");
        assert!(training_enabled());
    }

    #[test]
    fn gate_refused_when_env_unset() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::remove("XVN_ENABLE_LOCAL_TRAINING");
        assert!(!training_enabled());
    }

    #[test]
    fn gate_refused_when_env_set_to_0() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set("XVN_ENABLE_LOCAL_TRAINING", "0");
        assert!(!training_enabled());
    }

    #[test]
    fn gate_refused_when_env_set_to_false() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set("XVN_ENABLE_LOCAL_TRAINING", "false");
        assert!(!training_enabled());
    }

    #[test]
    fn require_training_returns_ok_when_enabled() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set("XVN_ENABLE_LOCAL_TRAINING", "1");
        assert!(require_training_enabled().is_ok());
    }

    #[test]
    fn require_training_returns_err_when_disabled() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::remove("XVN_ENABLE_LOCAL_TRAINING");
        let err = require_training_enabled().unwrap_err();
        assert!(err.to_string().contains("XVN_ENABLE_LOCAL_TRAINING"), "{err}");
    }

    /// RAII guard that sets/removes an env var and restores the prior value on drop.
    struct EnvGuard {
        key: &'static str,
        prior: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, prior }
        }

        fn remove(key: &'static str) -> Self {
            let prior = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, prior }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prior {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
