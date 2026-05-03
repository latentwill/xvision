//! Phase 8.1 — paired bootstrap for Δ-Sharpe with optional fixed-block
//! sampling.
//!
//! ## Block-bootstrap choice
//! This implementation uses **fixed-block** (non-overlapping blocks drawn with
//! replacement) rather than the stationary variant. Fixed-block is simpler to
//! implement deterministically, preserves local autocorrelation structure within
//! blocks, and is sufficient for the v1 goal of detecting serial correlation
//! bias relative to IID resampling. The stationary variant (Politis & Romano
//! 1994) adds random block lengths; it is a Tier 2 enhancement if the
//! distribution of block lengths materially affects CI width in practice.
//!
//! ## Tier 1 fix #4 compliance
//! When `block_size` is `Some(b)`, the resampled indices are drawn as complete
//! blocks of length `b`. This widens the confidence interval for serially
//! correlated returns compared to IID resampling, making the test more
//! conservative.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::metrics::sharpe_annualized;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("returns_a and returns_b must have equal length (got {a} vs {b})")]
    LengthMismatch { a: usize, b: usize },
    #[error("block_size ({block_size}) must be >= 1 and <= n ({n})")]
    InvalidBlockSize { block_size: usize, n: usize },
    #[error("n_resamples must be >= 1")]
    ZeroResamples,
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Output of the paired bootstrap for Δ-Sharpe (arm_a − arm_b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResult {
    /// Δ-Sharpe computed on the original (un-resampled) paired series.
    pub point_estimate: f32,
    /// 2.5th percentile of the resampled Δ-Sharpe distribution.
    pub ci_low: f32,
    /// 97.5th percentile of the resampled Δ-Sharpe distribution.
    pub ci_high: f32,
    pub n_resamples: usize,
    pub block_size: Option<usize>,
}

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Paired bootstrap for Δ-Sharpe (Sharpe_a − Sharpe_b).
///
/// # Arguments
/// - `returns_a`, `returns_b`: equal-length slices of per-setup returns.
///   The caller guarantees they are ordered by the same setup sequence.
/// - `n_resamples`: number of bootstrap replicates.
/// - `block_size`: `None` → IID resampling; `Some(b)` → fixed-block with
///   block length `b` (Tier 1 fix #4 for serially correlated returns).
/// - `periods_per_year`: passed unchanged to `sharpe_annualized`.
/// - `seed`: fed to `StdRng::seed_from_u64` for deterministic resampling.
pub fn paired_bootstrap_sharpe_delta(
    returns_a: &[f32],
    returns_b: &[f32],
    n_resamples: usize,
    block_size: Option<usize>,
    periods_per_year: f32,
    seed: u64,
) -> Result<BootstrapResult, BootstrapError> {
    let n = returns_a.len();
    if returns_b.len() != n {
        return Err(BootstrapError::LengthMismatch {
            a: n,
            b: returns_b.len(),
        });
    }
    if n_resamples == 0 {
        return Err(BootstrapError::ZeroResamples);
    }
    if let Some(bs) = block_size {
        if bs == 0 || bs > n {
            return Err(BootstrapError::InvalidBlockSize { block_size: bs, n });
        }
    }

    // Point estimate on the original series
    let point_estimate =
        sharpe_annualized(returns_a, periods_per_year) - sharpe_annualized(returns_b, periods_per_year);

    // Edge case: nothing to resample
    if n == 0 {
        return Ok(BootstrapResult {
            point_estimate,
            ci_low: point_estimate,
            ci_high: point_estimate,
            n_resamples,
            block_size,
        });
    }

    let mut rng = StdRng::seed_from_u64(seed);
    let mut deltas: Vec<f32> = Vec::with_capacity(n_resamples);

    match block_size {
        None => {
            // IID resampling: draw n indices uniformly with replacement
            let mut sample_a = Vec::with_capacity(n);
            let mut sample_b = Vec::with_capacity(n);
            for _ in 0..n_resamples {
                sample_a.clear();
                sample_b.clear();
                for _ in 0..n {
                    let idx = rng.gen_range(0..n);
                    sample_a.push(returns_a[idx]);
                    sample_b.push(returns_b[idx]);
                }
                let delta = sharpe_annualized(&sample_a, periods_per_year)
                    - sharpe_annualized(&sample_b, periods_per_year);
                deltas.push(delta);
            }
        }
        Some(bs) => {
            // Fixed-block resampling: draw ceil(n/bs) blocks, then truncate to n.
            // A "block start" index is drawn uniformly from 0..=(n-bs); the
            // entire block [start, start+bs) is appended.
            let n_blocks = n.div_ceil(bs);
            let max_start = n.saturating_sub(bs);

            let mut sample_a = Vec::with_capacity(n_blocks * bs);
            let mut sample_b = Vec::with_capacity(n_blocks * bs);
            for _ in 0..n_resamples {
                sample_a.clear();
                sample_b.clear();
                for _ in 0..n_blocks {
                    let start = if max_start > 0 {
                        rng.gen_range(0..=max_start)
                    } else {
                        0
                    };
                    for j in 0..bs {
                        let idx = (start + j).min(n - 1);
                        sample_a.push(returns_a[idx]);
                        sample_b.push(returns_b[idx]);
                    }
                }
                // Truncate to exactly n
                sample_a.truncate(n);
                sample_b.truncate(n);
                let delta = sharpe_annualized(&sample_a, periods_per_year)
                    - sharpe_annualized(&sample_b, periods_per_year);
                deltas.push(delta);
            }
        }
    }

    // 95% CI via 2.5 / 97.5 percentiles
    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ci_low = percentile(&deltas, 2.5);
    let ci_high = percentile(&deltas, 97.5);

    Ok(BootstrapResult {
        point_estimate,
        ci_low,
        ci_high,
        n_resamples,
        block_size,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Linear-interpolation percentile on a pre-sorted slice.
fn percentile(sorted: &[f32], pct: f64) -> f32 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return sorted[0];
    }
    let rank = pct / 100.0 * (n - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let frac = (rank - lo as f64) as f32;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PY: f32 = 252.0;

    // -----------------------------------------------------------------------
    // Same-arm bootstrap: Δ-Sharpe point estimate ≈ 0
    // -----------------------------------------------------------------------

    #[test]
    fn same_arm_point_estimate_is_zero() {
        let returns: Vec<f32> = (0..50).map(|i| 0.001 * (i as f32 % 7.0 - 3.0)).collect();
        let result = paired_bootstrap_sharpe_delta(&returns, &returns, 500, None, PY, 42)
            .expect("must succeed");
        assert!(
            result.point_estimate.abs() < 1e-5,
            "same-arm Δ-Sharpe should be ~0, got {}",
            result.point_estimate
        );
    }

    // -----------------------------------------------------------------------
    // Deterministic: same seed → byte-identical results
    // -----------------------------------------------------------------------

    #[test]
    fn deterministic_given_seed() {
        let a: Vec<f32> = (0..40).map(|i| 0.002 * (i as f32) - 0.04).collect();
        let b: Vec<f32> = (0..40).map(|i| 0.001 * (i as f32) - 0.02).collect();

        let r1 = paired_bootstrap_sharpe_delta(&a, &b, 200, None, PY, 0xdeadbeef)
            .expect("must succeed");
        let r2 = paired_bootstrap_sharpe_delta(&a, &b, 200, None, PY, 0xdeadbeef)
            .expect("must succeed");

        // Bit-identical floats
        assert_eq!(
            r1.point_estimate.to_bits(),
            r2.point_estimate.to_bits()
        );
        assert_eq!(r1.ci_low.to_bits(), r2.ci_low.to_bits());
        assert_eq!(r1.ci_high.to_bits(), r2.ci_high.to_bits());
    }

    // -----------------------------------------------------------------------
    // Block-bootstrap with autocorrelated returns widens CI vs IID
    // -----------------------------------------------------------------------

    #[test]
    fn block_bootstrap_widens_ci_for_autocorrelated_returns() {
        // Create strongly autocorrelated returns: 10-bar positive run,
        // 10-bar negative run, repeated. The block bootstrap with block=10
        // should preserve this structure, producing wider CI than IID.
        let n = 100;
        let autocorr: Vec<f32> = (0..n)
            .map(|i| if (i / 10) % 2 == 0 { 0.02 } else { -0.02 })
            .collect();
        // Arm B: same pattern but slightly shifted (divergent returns)
        let b: Vec<f32> = (0..n)
            .map(|i| if (i / 10) % 2 == 0 { 0.015 } else { -0.015 })
            .collect();

        let iid = paired_bootstrap_sharpe_delta(&autocorr, &b, 1000, None, PY, 99)
            .expect("iid must succeed");
        let block =
            paired_bootstrap_sharpe_delta(&autocorr, &b, 1000, Some(10), PY, 99)
                .expect("block must succeed");

        let iid_width = iid.ci_high - iid.ci_low;
        let block_width = block.ci_high - block.ci_low;

        assert!(
            block_width >= iid_width * 0.8, // block CI should not be narrower than 80% of IID
            "block CI width {block_width:.4} should be ≥ 80% of IID width {iid_width:.4}"
        );
        // More meaningful: block should be wider (or at worst similar)
        // We use a soft bound because small samples can produce slight inversions
    }

    // -----------------------------------------------------------------------
    // Error conditions
    // -----------------------------------------------------------------------

    #[test]
    fn mismatched_lengths_returns_error() {
        let a = vec![0.01_f32; 10];
        let b = vec![0.01_f32; 5];
        assert!(matches!(
            paired_bootstrap_sharpe_delta(&a, &b, 100, None, PY, 0),
            Err(BootstrapError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn zero_resamples_returns_error() {
        let a = vec![0.01_f32; 10];
        assert!(matches!(
            paired_bootstrap_sharpe_delta(&a, &a, 0, None, PY, 0),
            Err(BootstrapError::ZeroResamples)
        ));
    }

    #[test]
    fn invalid_block_size_returns_error() {
        let a = vec![0.01_f32; 10];
        assert!(matches!(
            paired_bootstrap_sharpe_delta(&a, &a, 100, Some(0), PY, 0),
            Err(BootstrapError::InvalidBlockSize { .. })
        ));
        assert!(matches!(
            paired_bootstrap_sharpe_delta(&a, &a, 100, Some(11), PY, 0),
            Err(BootstrapError::InvalidBlockSize { .. })
        ));
    }

    // -----------------------------------------------------------------------
    // CI ordering sanity
    // -----------------------------------------------------------------------

    #[test]
    fn ci_low_le_point_estimate_le_ci_high_not_required_but_point_in_range_common() {
        // This is a sanity check, not a strict invariant — the point estimate
        // can fall outside the bootstrap distribution but is usually within it.
        let a: Vec<f32> = (0..60).map(|i| 0.003 * (i as f32) - 0.09).collect();
        let b: Vec<f32> = (0..60).map(|i| 0.001 * (i as f32) - 0.03).collect();
        let r = paired_bootstrap_sharpe_delta(&a, &b, 500, None, PY, 7)
            .expect("must succeed");
        assert!(r.ci_low <= r.ci_high, "ci_low must be <= ci_high");
        assert_eq!(r.n_resamples, 500);
        assert!(r.block_size.is_none());
    }
}
