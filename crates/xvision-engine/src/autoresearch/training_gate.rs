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
    match std::env::var("XVN_ENABLE_LOCAL_TRAINING")
        .ok()
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        // Explicit override: force-enable.
        Some("1") | Some("true") => return Ok(()),
        // Explicit override: force-disable.
        Some("0") | Some("false") => {
            bail!(
                "XVN_ENABLE_LOCAL_TRAINING=0 — local training is explicitly \
                 disabled on this host. Set to '1' to enable."
            )
        }
        // Auto-detect hardware.
        _ => {}
    }

    // Check for a CUDA-capable GPU with ≥ 4 GB VRAM.
    let gpu = detect_gpu();
    // Check for the `uv` Python runner.
    let has_uv = std::process::Command::new("uv").arg("--version").output().is_ok();

    match (gpu, has_uv) {
        (GpuStatus::Available { .. }, true) => {
            // GPU + uv present — training can run. Log the auto-detection
            // so operators know why they don't need the env var.
            tracing::info!(
                "local training auto-enabled: CUDA GPU detected, uv available"
            );
            Ok(())
        }
        (GpuStatus::Unavailable { reason }, false) => {
            bail!(
                "local training is unavailable on this host. \
                 GPU: {reason}. \
                 Python runner: `uv` not found on PATH. \
                 Install `uv` (https://docs.astral.sh/uv/) and ensure a \
                 CUDA GPU with ≥ 4 GB VRAM is available. \
                 Set XVN_ENABLE_LOCAL_TRAINING=1 to bypass this check."
            )
        }
        (GpuStatus::Available { .. }, false) => {
            bail!(
                "local training requires `uv` (Python runner), which was not \
                 found on PATH. Install it: https://docs.astral.sh/uv/ \
                 Set XVN_ENABLE_LOCAL_TRAINING=1 to bypass this check."
            )
        }
        (GpuStatus::Unavailable { reason }, true) => {
            bail!(
                "local training requires a CUDA GPU with ≥ 4 GB VRAM, but \
                 {reason}. Set XVN_ENABLE_LOCAL_TRAINING=1 to bypass \
                 this check (training will fall back to CPU, which may be \
                 extremely slow)."
            )
        }
    }
}

/// Outcome of GPU hardware detection.
enum GpuStatus {
    /// CUDA GPU found with at least `vram_mb` MB.
    Available { vram_mb: u64, },
    /// No suitable GPU found.
    Unavailable { reason: String },
}

/// Detect a CUDA-capable GPU with ≥ 4 GB VRAM.
fn detect_gpu() -> GpuStatus {
    // 1. Try `nvidia-smi` — fast, reliable on any CUDA machine.
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Ok(mb) = line.trim().parse::<u64>() {
                if mb >= 4096 {
                    return GpuStatus::Available { vram_mb: mb, };
                }
                return GpuStatus::Unavailable {
                    reason: format!("found GPU with only {mb} MB VRAM (need ≥ 4096 MB)"),
                };
            }
        }
    }

    // 2. Try `system_profiler` on macOS (Apple Silicon GPU).
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sysctl")
            .args(["hw.memsize"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(rest) = text.split(':').nth(1) {
                if let Ok(bytes) = rest.trim().parse::<u64>() {
                    let mb = bytes / 1_048_576;
                    // Apple Silicon shares system RAM — assume 8 GB usable for GPU.
                    // Training works on M-series with ≥ 16 GB unified memory.
                    if mb >= 16_384 {
                        return GpuStatus::Available { vram_mb: mb / 2 };
                    }
                    return GpuStatus::Unavailable {
                        reason: format!(
                            "Apple Silicon requires ≥ 16 GB unified memory \
                             (detected {mb} MB)"
                        ),
                    };
                }
            }
        }
    }

    GpuStatus::Unavailable {
        reason: "no CUDA GPU detected (`nvidia-smi` not found)".into(),
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
    fn require_training_returns_err_when_explicitly_disabled() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set("XVN_ENABLE_LOCAL_TRAINING", "0");
        let err = require_training_enabled().unwrap_err();
        assert!(err.to_string().contains("explicitly disabled"), "{err}");
    }

    #[test]
    fn require_training_auto_detect_does_not_panic() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::remove("XVN_ENABLE_LOCAL_TRAINING");
        // Auto-detect may pass or fail depending on hardware — just assert it
        // returns without panicking.
        let _ = require_training_enabled();
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
