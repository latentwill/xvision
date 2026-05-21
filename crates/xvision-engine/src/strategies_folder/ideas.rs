//! Indexed read over `$XVN_HOME/strategies/library/templates/**/*.json` — the
//! curated strategy idea library prepopulated by `xvn strategies init` (see
//! `super::prepop`). The wizard reaches for this when the operator asks for
//! "examples" or "ideas" rather than enumerating their own raw notes.
//!
//! V2F closer track — `team/contracts/strategy-ideas-tool-surface.md` and
//! `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.
//!
//! ## Public surface
//!
//! - [`list_ideas`] enumerates the library and returns [`IdeaSummary`] rows.
//! - [`IdeaFilter`] is the input shape with optional `category`, `indicator`,
//!   and `limit` fields. All filtering is case-insensitive.
//!
//! ## Where the source data comes from
//!
//! Reads only from `<xvn_home>/strategies/library/templates/**/*.json`.
//! Each file is expected to be the `xvision.strategy_template.v1` shape
//! used by `docs/strategies/templates/**` (see `prepop.rs` for the
//! embedded source). Templates outside `library/templates/` are ignored.
//!
//! ## Filter semantics
//!
//! - `category` matches the immediate subfolder under `templates/`
//!   (e.g. `ema`, `bollinger`, `fibonacci`, `nansen`, `random`,
//!   `rsi-volume`). Match is case-insensitive against both the folder
//!   name and a normalized alias (so `ema` matches the on-disk `EMA`
//!   folder, `fibonacci` matches `FibonacciStrategy`, and `rsi-volume`
//!   matches `rsi_volume`). The normalization lowercases, strips
//!   non-alphanumeric chars, and drops a trailing `strategy` suffix.
//! - `indicator` matches any entry in the derived `indicators` vec on
//!   the parsed template (see [`derive_indicators`]). Case-insensitive
//!   substring match — `"rsi"` matches `"rsi_14"`, `"RSI"` matches
//!   `"indicator_panel.rsi_14"`, etc. This is intentionally loose so
//!   the wizard agent doesn't have to know the exact tag naming.
//! - `limit` defaults to 20 and is clamped to `MAX_LIMIT` (100). Negative
//!   / zero limits fall back to the default.
//!
//! ## Missing library + malformed entries
//!
//! - No `library/templates/` directory → `Ok(vec![])`. Operators who
//!   haven't run `xvn strategies init` get an empty result rather than
//!   an error.
//! - A JSON file that fails to parse the template shape is logged via
//!   `tracing::warn!` (target = `strategies_folder::ideas`) and skipped.
//!   The remainder of the library still surfaces, so a single bad file
//!   doesn't blackhole the entire wizard tool call.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiResult};

use super::reader::folder_root;

/// Default cap on returned ideas when no `limit` is supplied (or a
/// non-positive value is passed). Sized so the wizard model gets a
/// useful sample without being flooded.
pub const DEFAULT_LIMIT: u32 = 20;

/// Hard upper bound on `limit`. Requests over this are silently
/// clamped — the wizard agent doesn't need to babysit the cap.
pub const MAX_LIMIT: u32 = 100;

/// Subfolder under the strategies root that holds the curated template
/// library. Mirrors `prepop::library_rel_path_for` — both modules read
/// from the same path. Hardcoded here rather than re-exported because
/// the relationship is incidental, not a dependency.
const LIBRARY_TEMPLATES_REL: &str = "library/templates";

/// Filter passed to [`list_ideas`]. All fields optional; an empty filter
/// returns the first [`DEFAULT_LIMIT`] entries.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdeaFilter {
    /// Optional category (subfolder) filter. Case-insensitive; see
    /// module docs for the normalization rules.
    #[serde(default)]
    pub category: Option<String>,
    /// Optional indicator filter. Case-insensitive substring match
    /// against the derived `indicators` vec on each idea.
    #[serde(default)]
    pub indicator: Option<String>,
    /// Optional cap on the number of results. Defaults to
    /// [`DEFAULT_LIMIT`], clamped to [`MAX_LIMIT`]. Non-positive values
    /// fall back to the default.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// One row in the [`list_ideas`] response. Shaped for the wizard agent's
/// consumption — short, quotable, and points back at the original file
/// via `source_rel_path` so the agent can call `read_strategies_file`
/// to fetch the full template.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdeaSummary {
    /// Stable identifier — uses the template's `name` field
    /// (e.g. `ema_pullback_bounce`). Unique per category.
    pub id: String,
    /// Category, normalized to lowercase + hyphenated (e.g. `ema`,
    /// `rsi-volume`, `fibonacci`). Stable across renames of the
    /// underlying folder (see [`normalize_category`]).
    pub category: String,
    /// Indicators referenced by the template. Derived from the
    /// `required_tools` array plus any `IndicatorPanel.<name>` /
    /// `OnchainPanel.<name>` references in the `inputs` section.
    /// Lowercased + de-duplicated; order is the order discovered.
    pub indicators: Vec<String>,
    /// Display name. Falls back to `name` if `display_name` is missing.
    pub name: String,
    /// Short summary. Falls back through `plain_summary`,
    /// `sections.thesis`, then truncated `sections.decision_rule`.
    /// Whitespace-normalized and clipped to ~280 chars.
    pub summary: String,
    /// Path relative to `folder_root()` (forward-slash separated) that
    /// the agent can pass to `read_strategies_file` to fetch the full
    /// template.
    pub source_rel_path: String,
}

/// Enumerate strategy ideas under
/// `<xvn_home>/strategies/library/templates/**/*.json` and return summaries
/// filtered per [`IdeaFilter`]. See the module docs for behavior contract.
pub async fn list_ideas(ctx: &ApiContext, filter: IdeaFilter) -> ApiResult<Vec<IdeaSummary>> {
    let templates_root = folder_root(&ctx.xvn_home).join(LIBRARY_TEMPLATES_REL);

    // Missing library is the empty case, not an error. Operators who
    // haven't run `xvn strategies init` (a brand-new install, a fresh
    // container, etc) see an empty array — the wizard agent's prompt
    // is responsible for noticing and offering to run init.
    if !tokio::fs::try_exists(&templates_root).await.unwrap_or(false) {
        return Ok(Vec::new());
    }

    // Walk the templates tree. We collect file paths first (sync I/O
    // happens inside the walker via `tokio::fs::read_dir`) then parse
    // each JSON body. Done sequentially because the trees are small
    // (~50 files) and the wizard call is on a request path — burning
    // task::spawn fan-out for a 50-file walk doesn't pay back.
    let mut files: Vec<PathBuf> = Vec::new();
    collect_json_files(&templates_root, &mut files).await?;
    files.sort();

    let strategies_root = folder_root(&ctx.xvn_home);
    let mut ideas: Vec<IdeaSummary> = Vec::with_capacity(files.len());
    for path in files {
        match parse_idea(&path, &strategies_root, &templates_root).await {
            Ok(Some(idea)) => ideas.push(idea),
            Ok(None) => {} // file outside templates root — skipped silently
            Err(err) => {
                tracing::warn!(
                    target: "strategies_folder::ideas",
                    path = %path.display(),
                    error = %err,
                    "skipping malformed strategy idea template"
                );
            }
        }
    }

    // Filter. Normalization happens once per filter, not once per idea.
    let category_norm = filter
        .category
        .as_deref()
        .map(normalize_category)
        .filter(|s| !s.is_empty());
    let indicator_needle = filter
        .indicator
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());

    let filtered: Vec<IdeaSummary> = ideas
        .into_iter()
        .filter(|idea| match &category_norm {
            None => true,
            Some(needle) => normalize_category(&idea.category) == *needle,
        })
        .filter(|idea| match &indicator_needle {
            None => true,
            Some(needle) => idea
                .indicators
                .iter()
                .any(|ind| ind.to_ascii_lowercase().contains(needle)),
        })
        .collect();

    // Limit. `0` and absent → default. Above `MAX_LIMIT` → cap.
    let limit = match filter.limit {
        Some(n) if n > 0 => n.min(MAX_LIMIT),
        _ => DEFAULT_LIMIT,
    } as usize;

    Ok(filtered.into_iter().take(limit).collect())
}

/// Normalize a category name (subfolder slug or user-supplied filter
/// value) into a stable lowercased+alnum-only form. The pre-populated
/// `docs/strategies/templates/` folders use a mix of casing and
/// separators (`EMA`, `bollinger`, `FibonacciStrategy`, `rsi_volume`);
/// users will type any of `ema`, `EMA`, `Fibonacci`, `rsi-volume`.
/// Mapping both sides through this function makes the comparison
/// robust without baking in a hand-written alias table.
///
/// Rules:
/// - Lowercased.
/// - Non-alphanumeric characters dropped.
/// - Trailing `strategy` / `strategies` suffix dropped, so
///   `FibonacciStrategy` ↔ `fibonacci`.
///
/// Examples:
/// - `EMA` → `ema`
/// - `rsi-volume` → `rsivolume`
/// - `rsi_volume` → `rsivolume`
/// - `FibonacciStrategy` → `fibonacci`
/// - `Fibonacci` → `fibonacci`
fn normalize_category(input: &str) -> String {
    let mut s: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if let Some(stripped) = s.strip_suffix("strategies") {
        s = stripped.to_string();
    } else if let Some(stripped) = s.strip_suffix("strategy") {
        s = stripped.to_string();
    }
    s
}

/// Recursive walk that pushes every `.json` regular-file path it finds
/// under `root` into `out`. Hidden files (e.g. `.from-docs.json`) are
/// skipped so the manifest doesn't get parsed as a template.
async fn collect_json_files(root: &Path, out: &mut Vec<PathBuf>) -> ApiResult<()> {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&current).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(crate::api::ApiError::Internal(format!(
                    "read_dir {}: {e}",
                    current.display()
                )));
            }
        };
        while let Some(entry) = rd.next_entry().await.map_err(|e| {
            crate::api::ApiError::Internal(format!("read_dir next {}: {e}", current.display()))
        })? {
            let path = entry.path();
            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if file_name.starts_with('.') {
                // Skip hidden bookkeeping like `.from-docs.json`.
                continue;
            }
            let metadata = match tokio::fs::symlink_metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                == Some("json".to_string())
            {
                out.push(path);
            }
        }
    }
    Ok(())
}

/// Parse one template file into an [`IdeaSummary`]. Returns:
/// - `Ok(Some(_))` on a valid template (or any JSON we can extract the
///   minimum fields from — schema version isn't enforced because a
///   couple of curated templates omit it).
/// - `Ok(None)` if the path resolved outside the templates root after
///   canonicalization (defense in depth against symlink escapes).
/// - `Err(_)` only for I/O / JSON parse failures the caller wants to
///   log + skip. Schema-shape errors bubble through here too.
async fn parse_idea(
    path: &Path,
    strategies_root: &Path,
    templates_root: &Path,
) -> ApiResult<Option<IdeaSummary>> {
    let canonical_path = tokio::fs::canonicalize(path)
        .await
        .map_err(|e| crate::api::ApiError::Internal(format!("canonicalize {}: {e}", path.display())))?;
    let canonical_strategies_root = tokio::fs::canonicalize(strategies_root).await.map_err(|e| {
        crate::api::ApiError::Internal(format!("canonicalize {}: {e}", strategies_root.display()))
    })?;
    let canonical_templates_root = tokio::fs::canonicalize(templates_root).await.map_err(|e| {
        crate::api::ApiError::Internal(format!("canonicalize {}: {e}", templates_root.display()))
    })?;
    if !canonical_path.starts_with(&canonical_templates_root) {
        return Ok(None);
    }

    let bytes = tokio::fs::read(&canonical_path)
        .await
        .map_err(|e| crate::api::ApiError::Internal(format!("read {}: {e}", canonical_path.display())))?;
    let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        crate::api::ApiError::Internal(format!("parse json {}: {e}", canonical_path.display()))
    })?;

    // Use the path-relative-to-strategies-root as the source_rel_path
    // (`library/templates/EMA/ema_pullback_bounce.json`). We also need
    // the path-relative-to-templates-root to derive the category from
    // the first path component (`EMA`).
    let rel_to_strategies = match canonical_path.strip_prefix(&canonical_strategies_root) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let rel_to_templates = match canonical_path.strip_prefix(&canonical_templates_root) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let source_rel_path = rel_to_strategies
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");

    // Category = first path component under templates/. If the file is
    // sitting loose directly under templates/ (no subfolder), treat the
    // category as the stem of the file itself rather than the empty
    // string — keeps the row at least filterable.
    let category_raw = rel_to_templates
        .components()
        .find_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
            _ => None,
        })
        .unwrap_or_default();
    let category = category_label(&category_raw);

    // Minimum required field: `name`. Without it the row has no stable
    // id, so we treat it as malformed and let the caller log+skip.
    let name_field = raw
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::api::ApiError::Validation(format!(
                "template {} missing required `name` field",
                path.display()
            ))
        })?
        .to_string();

    let display_name = raw
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| name_field.clone());

    let summary = derive_summary(&raw);
    let indicators = derive_indicators(&raw);

    Ok(Some(IdeaSummary {
        id: name_field,
        category,
        indicators,
        name: display_name,
        summary,
        source_rel_path,
    }))
}

/// Convert a raw folder name into a user-friendly category label.
/// Lowercases, hyphenates underscores, and strips trailing `strategy` /
/// `strategies` so `FibonacciStrategy` surfaces as `fibonacci`.
///
/// Keeping the label as a single hyphenated token (rather than free-form)
/// lets the wizard agent quote it back into a follow-up filter without
/// quoting subtleties.
fn category_label(folder: &str) -> String {
    let lowered = folder.to_ascii_lowercase().replace('_', "-");
    if let Some(stripped) = lowered.strip_suffix("strategy") {
        return stripped.trim_end_matches('-').to_string();
    }
    if let Some(stripped) = lowered.strip_suffix("strategies") {
        return stripped.trim_end_matches('-').to_string();
    }
    lowered
}

/// Derive a one-line-ish summary from a template, picking the most
/// concrete field available and clipping to ~280 chars (Twitter-sized,
/// also fits comfortably into a wizard `tool_result` for downstream
/// model context).
///
/// Preference order: `description` (per the contract — not actually
/// present in current templates but supported for forward compat) →
/// `plain_summary` → `sections.thesis` → first 280 chars of
/// `sections.decision_rule`. Whitespace is collapsed so the agent
/// sees a clean single-paragraph string.
fn derive_summary(raw: &serde_json::Value) -> String {
    let candidates = [
        raw.get("description").and_then(|v| v.as_str()),
        raw.get("plain_summary").and_then(|v| v.as_str()),
        raw.get("sections")
            .and_then(|s| s.get("thesis"))
            .and_then(|v| v.as_str()),
        raw.get("sections")
            .and_then(|s| s.get("decision_rule"))
            .and_then(|v| v.as_str()),
    ];
    for cand in candidates.iter().flatten() {
        let normalized = normalize_whitespace(cand);
        if !normalized.is_empty() {
            return clip_chars(&normalized, 280);
        }
    }
    String::new()
}

/// Collapse runs of whitespace into single spaces and trim. Markdown
/// fences, indentation, and CR/LF noise from the source templates get
/// flattened into something readable in a single tool_result line.
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate to at most `max_chars` Unicode codepoints, appending `…`
/// when we actually clipped. Char-based (not byte-based) so we never
/// slice through a multibyte sequence.
fn clip_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

/// Pull a deduplicated list of indicator-ish tokens from a template:
/// 1. Every entry in `required_tools` (e.g. `ohlcv`, `indicator_panel`).
/// 2. Every `IndicatorPanel.<name>` / `OnchainPanel.<name>` reference
///    in the `sections.inputs` text — gives us `rsi_14`, `ema_200`,
///    `funding_rate_8h`, etc.
///
/// Lowercased and order-preserving so the wizard can quote them back.
fn derive_indicators(raw: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let push = |tok: &str, out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>| {
        let tok = tok.trim().to_ascii_lowercase();
        if tok.is_empty() {
            return;
        }
        if seen.insert(tok.clone()) {
            out.push(tok);
        }
    };

    if let Some(arr) = raw.get("required_tools").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(s) = entry.as_str() {
                push(s, &mut out, &mut seen);
            }
        }
    }

    if let Some(inputs) = raw
        .get("sections")
        .and_then(|s| s.get("inputs"))
        .and_then(|v| v.as_str())
    {
        for tok in extract_panel_refs(inputs) {
            push(&tok, &mut out, &mut seen);
        }
    }

    out
}

/// Scan a free-form `inputs` body for `IndicatorPanel.<name>` and
/// `OnchainPanel.<name>` references and return the `<name>` portion.
/// Implemented as a simple state machine rather than a regex — keeps
/// the crate dep-free and these patterns are tight enough to read in
/// one pass.
fn extract_panel_refs(text: &str) -> Vec<String> {
    const PANELS: &[&str] = &["IndicatorPanel.", "OnchainPanel.", "PriceFrame."];
    let mut out: Vec<String> = Vec::new();
    for panel in PANELS {
        let mut rest = text;
        while let Some(idx) = rest.find(panel) {
            let after = &rest[idx + panel.len()..];
            // Indicator name = leading run of [A-Za-z0-9_].
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            if end > 0 {
                out.push(after[..end].to_string());
            }
            rest = &after[end..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_category_handles_known_aliases() {
        assert_eq!(normalize_category("EMA"), "ema");
        assert_eq!(normalize_category("ema"), "ema");
        assert_eq!(normalize_category("rsi-volume"), "rsivolume");
        assert_eq!(normalize_category("rsi_volume"), "rsivolume");
        assert_eq!(normalize_category("FibonacciStrategy"), "fibonacci");
        assert_eq!(normalize_category("Fibonacci"), "fibonacci");
        assert_eq!(normalize_category("Bollinger"), "bollinger");
    }

    #[test]
    fn category_label_normalizes_folder_to_user_facing_string() {
        assert_eq!(category_label("EMA"), "ema");
        assert_eq!(category_label("rsi_volume"), "rsi-volume");
        assert_eq!(category_label("FibonacciStrategy"), "fibonacci");
        assert_eq!(category_label("bollinger"), "bollinger");
    }

    #[test]
    fn extract_panel_refs_pulls_indicator_panel_dot_tokens() {
        let body = "- `IndicatorPanel.rsi_14`\n- `IndicatorPanel.ema_200`\n- \
                    `OnchainPanel.funding_rate_8h`\n- `PriceFrame.close`";
        let got = extract_panel_refs(body);
        assert_eq!(got, vec!["rsi_14", "ema_200", "funding_rate_8h", "close"]);
    }

    #[test]
    fn clip_chars_preserves_unicode_boundaries() {
        let s = "é".repeat(400);
        let out = clip_chars(&s, 50);
        assert_eq!(out.chars().count(), 50);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn derive_summary_prefers_description_then_plain_summary_then_thesis() {
        let raw = serde_json::json!({
            "description": "  desc  body  ",
            "plain_summary": "ignored",
            "sections": { "thesis": "also ignored" }
        });
        assert_eq!(derive_summary(&raw), "desc body");

        let raw2 = serde_json::json!({
            "plain_summary": " p  s ",
            "sections": { "thesis": "ignored" }
        });
        assert_eq!(derive_summary(&raw2), "p s");

        let raw3 = serde_json::json!({
            "sections": { "thesis": "  thesis  text " }
        });
        assert_eq!(derive_summary(&raw3), "thesis text");
    }
}
