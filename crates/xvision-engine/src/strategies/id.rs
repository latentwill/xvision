//! Strategy ID validation for filesystem path safety.
//!
//! Remediation for QA finding P3-strategy-id (`qa/2026-05-17-comprehensive-codebase-review.md`,
//! item #5): `FilesystemStore::path_for` previously joined an arbitrary
//! caller-supplied string into a filesystem path under
//! `$XVN_HOME/strategies/`. Any caller that reached the store with an ID
//! containing `..` or a path separator could traverse to a sibling
//! `.json` file the operator did not intend to expose, write, or delete.
//!
//! The fix is an input invariant at the lowest layer: every `FilesystemStore`
//! method validates its `id` argument through [`validate_strategy_id_for_path`]
//! before any path join happens. The regex is intentionally strict —
//! `^[A-Za-z0-9_-]+$` — because every legitimate strategy ID in the
//! codebase today is either a ULID (alphanumeric upper-case Crockford)
//! or a hand-typed short slug, and there's no use case for separators
//! or dots in a strategy filename stem.
//!
//! Existing on-disk strategies are not migrated; ULID-shaped IDs already
//! satisfy the pattern.

use thiserror::Error;

/// Reasons a strategy ID is not safe to use as a filesystem stem.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StrategyIdError {
    #[error("strategy id is empty")]
    Empty,
    #[error("strategy id contains a path separator")]
    PathSeparator,
    #[error("strategy id contains a NUL byte")]
    NulByte,
    #[error("strategy id starts with a dot")]
    LeadingDot,
    #[error("strategy id is reserved ('.' or '..')")]
    ReservedSegment,
    #[error("strategy id contains disallowed character '{0}' (allowed: A-Z a-z 0-9 _ -)")]
    DisallowedChar(char),
}

/// Validate that `id` is safe to use as a filesystem stem under the
/// strategy store root.
///
/// Returns the borrowed `id` on success so the validator can be chained
/// inline: `let safe = validate_strategy_id_for_path(id)?;`.
///
/// # Pattern
///
/// `^[A-Za-z0-9_-]+$` — non-empty ASCII alphanumeric, underscore, or
/// hyphen. Explicit rejections (for clearer error messages than the
/// regex would give) cover NUL, path separators, leading dots, and the
/// reserved `.` / `..` segments. The fixed `.json` suffix appended by
/// `FilesystemStore::path_for` does NOT defend against traversal alone
/// — `../foo` resolves to `../foo.json`, which is still a sibling
/// directory escape — so the separator check is load-bearing.
pub fn validate_strategy_id_for_path(id: &str) -> Result<&str, StrategyIdError> {
    if id.is_empty() {
        return Err(StrategyIdError::Empty);
    }
    // Check path separators and NUL first so a payload like `../escape`
    // reports as a separator violation (the load-bearing one) rather
    // than the LeadingDot it would also satisfy.
    for ch in id.chars() {
        match ch {
            '\0' => return Err(StrategyIdError::NulByte),
            '/' | '\\' => return Err(StrategyIdError::PathSeparator),
            _ => {}
        }
    }
    if id == "." || id == ".." {
        return Err(StrategyIdError::ReservedSegment);
    }
    if id.starts_with('.') {
        return Err(StrategyIdError::LeadingDot);
    }
    for ch in id.chars() {
        match ch {
            c if c.is_ascii_alphanumeric() => continue,
            '_' | '-' => continue,
            other => return Err(StrategyIdError::DisallowedChar(other)),
        }
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ulid_shaped_id() {
        // Crockford base32 uppercase — the canonical strategy ID shape.
        let id = "01HZSTRATEGY00000000000000";
        assert_eq!(validate_strategy_id_for_path(id), Ok(id));
    }

    #[test]
    fn accepts_lowercase_alphanumeric_with_underscore_and_hyphen() {
        let id = "btc-momentum_v2";
        assert_eq!(validate_strategy_id_for_path(id), Ok(id));
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(validate_strategy_id_for_path(""), Err(StrategyIdError::Empty));
    }

    #[test]
    fn rejects_double_dot_traversal() {
        assert_eq!(
            validate_strategy_id_for_path(".."),
            Err(StrategyIdError::ReservedSegment),
        );
    }

    #[test]
    fn rejects_single_dot() {
        assert_eq!(
            validate_strategy_id_for_path("."),
            Err(StrategyIdError::ReservedSegment),
        );
    }

    #[test]
    fn rejects_forward_slash_traversal() {
        assert_eq!(
            validate_strategy_id_for_path("../etc/passwd"),
            Err(StrategyIdError::PathSeparator),
        );
    }

    #[test]
    fn rejects_backslash_traversal() {
        assert_eq!(
            validate_strategy_id_for_path("..\\windows\\system32"),
            Err(StrategyIdError::PathSeparator),
        );
    }

    #[test]
    fn rejects_leading_dot_hidden_file() {
        assert_eq!(
            validate_strategy_id_for_path(".hidden"),
            Err(StrategyIdError::LeadingDot),
        );
    }

    #[test]
    fn rejects_nul_byte() {
        assert_eq!(
            validate_strategy_id_for_path("foo\0bar"),
            Err(StrategyIdError::NulByte),
        );
    }

    #[test]
    fn rejects_whitespace() {
        assert_eq!(
            validate_strategy_id_for_path("foo bar"),
            Err(StrategyIdError::DisallowedChar(' ')),
        );
    }

    #[test]
    fn rejects_embedded_dot() {
        assert_eq!(
            validate_strategy_id_for_path("foo.bar"),
            Err(StrategyIdError::DisallowedChar('.')),
        );
    }

    #[test]
    fn rejects_unicode_letters() {
        assert!(matches!(
            validate_strategy_id_for_path("strätegy"),
            Err(StrategyIdError::DisallowedChar(_)),
        ));
    }
}
