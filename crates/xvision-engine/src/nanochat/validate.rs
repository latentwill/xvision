use std::path::{Component, Path, PathBuf};

/// Validates a `run_tag` string: must be 1–32 chars, start with [a-z0-9],
/// and contain only [a-z0-9-]. Equivalent to `^[a-z0-9][a-z0-9-]{0,31}$`.
/// The tag flows into a git branch name, a worktree path, and display strings.
pub fn validate_run_tag(tag: &str) -> Result<(), String> {
    // Rules (mirrors ^[a-z0-9][a-z0-9-]{0,31}$):
    // 1. Non-empty.
    // 2. Total length 1–32 chars.
    // 3. First char must be ascii lowercase alphanumeric (a-z, 0-9).
    // 4. Every subsequent char must be ascii lowercase alphanumeric or '-'.
    if tag.is_empty() {
        return Err(format!(
            "run_tag is empty: must be 1–32 chars, start with [a-z0-9], contain only [a-z0-9-]"
        ));
    }
    if tag.len() > 32 {
        return Err(format!(
            "run_tag {tag:?} is too long ({} chars): max 32",
            tag.len()
        ));
    }
    let mut chars = tag.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() || first.is_ascii_uppercase() {
        return Err(format!(
            "run_tag {tag:?}: first char must be ascii lowercase alphanumeric, got {first:?}"
        ));
    }
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() && !ch.is_ascii_uppercase()) && ch != '-' {
            return Err(format!(
                "run_tag {tag:?}: invalid char {ch:?}; only [a-z0-9-] allowed"
            ));
        }
    }
    Ok(())
}

/// Validates that `path` resolves to a location strictly under `models_dir`.
/// Rejects path traversal (`../`), symlink-free canonical comparison.
///
/// `models_dir` is the value of `XVN_NANOCHAT_MODELS_DIR`.
pub fn validate_checkpoint_path(path: &str, models_dir: &Path) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    // Canonicalize without touching the filesystem: resolve `.` and `..`
    // components lexically. We use `Path::components` to strip `.` and
    // accumulate the normalized segments.
    let normalized = normalize_path_lexical(&candidate);
    let base = normalize_path_lexical(models_dir);

    // Must start with base AND have at least one additional component.
    if normalized.starts_with(&base) && normalized != base {
        Ok(normalized)
    } else {
        Err(format!(
            "checkpoint_path {path:?} is outside the allowed models dir {:?}",
            models_dir
        ))
    }
}

/// Lexically normalize a path: resolve `.` and `..` without touching the
/// filesystem (no `std::fs::canonicalize`). Preserves absolute-path prefix.
fn normalize_path_lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                // Only pop if we have a non-root component to pop.
                if matches!(out.components().last(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Configuration for the promotion gate.
#[derive(Debug, Clone, Copy)]
pub struct PromotionGateCfg {
    /// Required improvement over current best (e.g. 0.01 = 1%).
    pub epsilon: f64,
    /// Minimum absolute accuracy floor (e.g. 0.52).
    pub acc_floor: f64,
    /// Minimum number of holdout samples behind the metric.
    pub min_holdout: i64,
}

impl Default for PromotionGateCfg {
    fn default() -> Self {
        Self {
            epsilon: 0.01,
            acc_floor: 0.52,
            min_holdout: 200,
        }
    }
}

/// Returns `true` iff the experiment should be promoted.
///
/// Promotes iff ALL hold:
/// 1. `val_acc` is not None (crash experiments never promote).
/// 2. `val_acc - current_best > cfg.epsilon` (beats current best by margin).
/// 3. `val_acc >= cfg.acc_floor` (meets absolute floor).
/// 4. `holdout_samples >= cfg.min_holdout` (backed by enough data).
///
/// When `current_best` is None (no prior promoted model), condition 2 requires
/// only `val_acc > cfg.epsilon` (i.e. `val_acc - 0.0 > epsilon`).
pub fn evaluate_promotion_gate(
    val_acc: Option<f64>,
    current_best: Option<f64>,
    holdout_samples: i64,
    cfg: PromotionGateCfg,
) -> bool {
    let acc = match val_acc {
        None => return false, // crash — never promote
        Some(v) => v,
    };
    let baseline = current_best.unwrap_or(0.0);
    acc - baseline > cfg.epsilon      // beats current best by margin
        && acc >= cfg.acc_floor       // meets absolute floor
        && holdout_samples >= cfg.min_holdout  // backed by enough data
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Task 2.1: validate_run_tag ──────────────────────────────────────────

    #[test]
    fn run_tag_valid_cases() {
        for tag in &["a", "abc", "jun12a", "a-b-c", "run-2026-06-14",
                      "a0", "0a", "abc-123-def", "a".repeat(32).as_str()] {
            assert!(validate_run_tag(tag).is_ok(), "expected valid: {tag}");
        }
    }

    #[test]
    fn run_tag_invalid_empty() {
        assert!(validate_run_tag("").is_err());
    }

    #[test]
    fn run_tag_invalid_starts_with_hyphen() {
        assert!(validate_run_tag("-abc").is_err());
    }

    #[test]
    fn run_tag_invalid_uppercase() {
        assert!(validate_run_tag("ABC").is_err());
    }

    #[test]
    fn run_tag_invalid_too_long() {
        // 33 chars: one start char + 32 suffix chars > limit of 32 total.
        assert!(validate_run_tag(&"a".repeat(33)).is_err());
    }

    #[test]
    fn run_tag_invalid_special_chars() {
        assert!(validate_run_tag("abc_def").is_err());
        assert!(validate_run_tag("abc.def").is_err());
        assert!(validate_run_tag("abc def").is_err());
    }

    #[test]
    fn run_tag_exactly_32_chars_is_valid() {
        // 1 start char + 31 suffix chars = 32 total = at the limit.
        let tag = "a".to_string() + &"b".repeat(31);
        assert_eq!(tag.len(), 32);
        assert!(validate_run_tag(&tag).is_ok());
    }

    // ── Task 2.2: validate_checkpoint_path ─────────────────────────────────

    #[test]
    fn checkpoint_path_valid_under_models_dir() {
        let base = PathBuf::from("/models/nanochat");
        assert!(validate_checkpoint_path("/models/nanochat/jun12a", &base).is_ok());
        assert!(validate_checkpoint_path("/models/nanochat/run-2026/checkpoint", &base).is_ok());
    }

    #[test]
    fn checkpoint_path_traversal_rejected() {
        let base = PathBuf::from("/models/nanochat");
        assert!(validate_checkpoint_path("/models/nanochat/../etc/passwd", &base).is_err());
        assert!(validate_checkpoint_path("../etc/passwd", &base).is_err());
        assert!(validate_checkpoint_path("/etc/passwd", &base).is_err());
    }

    #[test]
    fn checkpoint_path_outside_base_rejected() {
        let base = PathBuf::from("/models/nanochat");
        assert!(validate_checkpoint_path("/models/other/checkpoint", &base).is_err());
    }

    #[test]
    fn checkpoint_path_equal_to_base_rejected() {
        // The base dir itself is not a valid checkpoint path — must be a subpath.
        let base = PathBuf::from("/models/nanochat");
        assert!(validate_checkpoint_path("/models/nanochat", &base).is_err());
    }

    // ── Task 2.5: evaluate_promotion_gate ──────────────────────────────────

    #[test]
    fn null_val_acc_never_promotes() {
        assert!(!evaluate_promotion_gate(None, None, 300, PromotionGateCfg::default()));
        assert!(!evaluate_promotion_gate(None, Some(0.5), 300, PromotionGateCfg::default()));
    }

    #[test]
    fn promotes_when_all_conditions_hold() {
        // val_acc=0.55 > current_best(0.51) + epsilon(0.01)=0.52; floor=0.52; holdout=250 >= 200.
        assert!(evaluate_promotion_gate(
            Some(0.55),
            Some(0.51),
            250,
            PromotionGateCfg { epsilon: 0.01, acc_floor: 0.52, min_holdout: 200 },
        ));
    }

    #[test]
    fn fails_epsilon_condition() {
        // val_acc=0.53 - current_best(0.53) = 0.0 which is NOT > epsilon(0.01).
        assert!(!evaluate_promotion_gate(
            Some(0.53),
            Some(0.53),
            250,
            PromotionGateCfg::default(),
        ));
    }

    #[test]
    fn fails_floor_condition() {
        // val_acc=0.51 < floor=0.52 even if it beats current_best=None.
        assert!(!evaluate_promotion_gate(
            Some(0.51),
            None,
            250,
            PromotionGateCfg { epsilon: 0.01, acc_floor: 0.52, min_holdout: 200 },
        ));
    }

    #[test]
    fn fails_min_holdout_condition() {
        // Only 150 holdout samples < min_holdout=200.
        assert!(!evaluate_promotion_gate(
            Some(0.58),
            Some(0.50),
            150,
            PromotionGateCfg { epsilon: 0.01, acc_floor: 0.52, min_holdout: 200 },
        ));
    }

    #[test]
    fn no_prior_best_uses_zero_as_baseline() {
        // current_best=None → baseline=0.0.
        // val_acc=0.55; 0.55 - 0.0 = 0.55 > epsilon=0.01; floor=0.52 ok; holdout=300 ok.
        assert!(evaluate_promotion_gate(
            Some(0.55),
            None,
            300,
            PromotionGateCfg::default(),
        ));
    }

    #[test]
    fn just_at_floor_boundary_promotes() {
        // val_acc == floor (0.52) should pass (>= not >).
        assert!(evaluate_promotion_gate(
            Some(0.52),
            None,
            300,
            PromotionGateCfg { epsilon: 0.01, acc_floor: 0.52, min_holdout: 200 },
        ));
    }

    #[test]
    fn just_below_floor_does_not_promote() {
        assert!(!evaluate_promotion_gate(
            Some(0.5199),
            None,
            300,
            PromotionGateCfg { epsilon: 0.0, acc_floor: 0.52, min_holdout: 200 },
        ));
    }
}
