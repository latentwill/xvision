use std::path::{Path, PathBuf};

use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::strategies::id::{validate_strategy_id_for_path, StrategyIdError};
use crate::strategies::Strategy;
use xvision_filters::ActivationMode;

/// Partial-update body for [`update_metadata`]. All fields are
/// optional; `None` means "leave the existing value unchanged". A
/// patch with every field `None` is a valid no-op and round-trips the
/// stored strategy untouched.
///
/// Scope: the operator-editable top-level manifest fields a typo in the
/// create wizard could land on, plus `creator` (so the operator can stamp a
/// strategy with their profile handle — QA). The strategy `id`, `template`,
/// `published_at`, `risk_preset_or_config`, `agents`, `pipeline`, and `risk`
/// remain out of scope — they either have dedicated sub-routes
/// (slot/agents/pipeline/risk) or are immutable post-create.
///
/// # Color clear convention
///
/// `color: Some("")` (empty string) is the explicit "clear the color"
/// signal. The apply function maps empty string → `None`, erasing
/// whatever was stored. This lets the wire format stay
/// `Option<String>` (no separate `null` vs. `""` ambiguity) while
/// giving the UI a clean "unset" affordance.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrategyMetadataPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_universe: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_cadence_minutes: Option<u32>,
    /// Optional per-strategy display color. Must be a 7-character CSS
    /// hex string (`#RRGGBB`, case-insensitive) when non-empty.
    ///
    /// `Some("")` (empty string) explicitly clears the stored color
    /// (maps to `manifest.color = None`). `None` leaves the existing
    /// color untouched. `Some("#D4A547")` sets the color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Optional strategy author/owner handle. `Some(non-empty)` sets the
    /// `creator` (e.g. the operator's profile handle); `Some("")`/whitespace
    /// and `None` both leave the existing creator untouched (no accidental
    /// wipe). QA: "allow creator to be updated with the user profile".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
}

/// Structured errors from [`update_metadata`]. Each variant maps to
/// an operator-readable remediation message; the dashboard error
/// classifier surfaces these as `400 validation` rather than `500
/// internal` (#256 convention).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetadataPatchError {
    #[error("display_name cannot be empty — provide a non-blank title")]
    EmptyDisplayName,
    #[error("plain_summary cannot be empty — provide a non-blank description or omit the field")]
    EmptyPlainSummary,
    #[error("asset_universe cannot be empty — provide at least one asset symbol or omit the field")]
    EmptyAssetUniverse,
    #[error("asset_universe cannot include blank entries — drop the empty value or omit the field")]
    BlankAssetEntry,
    #[error("asset_universe entry '{0}' is not a valid asset symbol — expected SYMBOL/QUOTE (e.g. BTC/USD)")]
    InvalidAssetSymbol(String),
    #[error("decision_cadence_minutes must be greater than 0")]
    InvalidDecisionCadence,
    #[error("color '{0}' is not a valid hex color — expected 7-character CSS hex (e.g. #D4A547)")]
    InvalidColor(String),
}

/// Validates a non-empty color string against the CSS hex format `^#[0-9a-fA-F]{6}$`.
/// Returns `Ok(value)` unchanged on success, or `Err(InvalidColor)` on failure.
/// An empty string is not valid and should be converted to `None` before reaching here.
fn validate_hex_color(value: &str) -> Result<(), MetadataPatchError> {
    let bytes = value.as_bytes();
    if bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(MetadataPatchError::InvalidColor(value.to_string()))
    }
}

/// Returns `Ok(normalized_assets)` if every entry is a recognizable
/// asset symbol of the form `BASE/QUOTE`. Trims whitespace and
/// uppercases the result for storage. De-duplicates while preserving
/// first-seen order.
///
/// Kept here (rather than in `validate.rs`) because the per-entry
/// AssetSymbol shape check is patch-time validation, not the
/// holistic strategy-shape validation `validate_strategy` runs at
/// agent-pipeline acceptance.
fn normalize_asset_universe(input: Vec<String>) -> Result<Vec<String>, MetadataPatchError> {
    let mut out: Vec<String> = Vec::with_capacity(input.len());
    for raw in input {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(MetadataPatchError::BlankAssetEntry);
        }
        // BASE/QUOTE shape. Both halves must be non-empty,
        // alphanumeric. This matches what the existing prompt /
        // manifest drift checker recognizes as an asset token.
        let (base, quote) = trimmed
            .split_once('/')
            .ok_or_else(|| MetadataPatchError::InvalidAssetSymbol(trimmed.to_string()))?;
        let base = base.trim();
        let quote = quote.trim();
        if base.is_empty()
            || quote.is_empty()
            || !base.chars().all(|c| c.is_ascii_alphanumeric())
            || !quote.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(MetadataPatchError::InvalidAssetSymbol(trimmed.to_string()));
        }
        let normalized = format!("{}/{}", base.to_ascii_uppercase(), quote.to_ascii_uppercase());
        if !out.iter().any(|existing| existing == &normalized) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        return Err(MetadataPatchError::EmptyAssetUniverse);
    }
    Ok(out)
}

/// Apply a [`StrategyMetadataPatch`] to `strategy` in place. Returns
/// `Ok(())` if every supplied field validates; otherwise returns the
/// first encountered [`MetadataPatchError`] without mutating the
/// strategy.
///
/// Two-phase: validate every Some-field first (so a partial patch
/// failure doesn't leave the strategy half-updated), then apply.
pub fn apply_metadata_patch(
    strategy: &mut Strategy,
    patch: StrategyMetadataPatch,
) -> Result<(), MetadataPatchError> {
    // ── phase 1: validate every provided field ────────────────────
    let display_name = patch
        .display_name
        .map(|name| {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                Err(MetadataPatchError::EmptyDisplayName)
            } else {
                Ok(trimmed.to_string())
            }
        })
        .transpose()?;

    let plain_summary = patch
        .plain_summary
        .map(|summary| {
            let trimmed = summary.trim();
            if trimmed.is_empty() {
                Err(MetadataPatchError::EmptyPlainSummary)
            } else {
                Ok(trimmed.to_string())
            }
        })
        .transpose()?;

    let asset_universe = patch.asset_universe.map(normalize_asset_universe).transpose()?;
    let decision_cadence_minutes = match patch.decision_cadence_minutes {
        Some(0) => return Err(MetadataPatchError::InvalidDecisionCadence),
        other => other,
    };

    // color: None → leave unchanged; Some("") → clear (map to None);
    // Some(non-empty) → validate hex and store.
    let color_action: Option<Option<String>> = match patch.color {
        None => None,
        Some(ref s) if s.is_empty() => Some(None), // explicit clear
        Some(ref s) => {
            validate_hex_color(s)?;
            Some(Some(s.clone()))
        }
    };

    // ── phase 2: apply ────────────────────────────────────────────
    if let Some(name) = display_name {
        strategy.manifest.display_name = name;
    }
    if let Some(summary) = plain_summary {
        strategy.manifest.plain_summary = summary;
    }
    if let Some(assets) = asset_universe {
        strategy.manifest.asset_universe = assets;
    }
    if let Some(cadence) = decision_cadence_minutes {
        strategy.manifest.decision_cadence_minutes = cadence;
    }
    if let Some(color) = color_action {
        strategy.manifest.color = color;
    }
    // creator: Some(non-empty trimmed) → set; Some("")/whitespace and None →
    // leave untouched (no accidental wipe of authorship).
    if let Some(creator) = patch.creator.as_deref() {
        let trimmed = creator.trim();
        if !trimmed.is_empty() {
            strategy.manifest.creator = trimmed.to_string();
        }
    }
    Ok(())
}

/// Pre-persist validation seam used by every [`StrategyStore`]
/// implementation.
///
/// Before the 2026-05-21 template-registry removal this ran an F-6
/// typed parse against `manifest.template` to catch
/// `deny_unknown_fields` violations that bypassed the deserialize
/// boundary via direct struct construction. With the template registry
/// gone there is no per-strategy schema to dispatch against, so the
/// seam is currently a no-op. Kept as a seam so the V2F per-strategy
/// schema work (declared per seed in `docs/strategies/templates/`) has
/// a fixed place to slot in.
///
/// Public so alternative `StrategyStore` impls (in-memory stubs,
/// future remote stores) can call the same seam instead of
/// re-deriving the checks.
pub fn validate_strategy_for_persist(strategy: &Strategy) -> anyhow::Result<()> {
    if strategy.activation_mode == ActivationMode::FilterGated && strategy.filter.is_none() {
        anyhow::bail!(
            "activation_mode is filter_gated but filter is None — \
             the filter block failed to load or is missing from the file"
        );
    }
    Ok(())
}

/// Canonical on-disk directory for `Strategy` JSON files, relative to
/// `$XVN_HOME`. Single source of truth so the CLI and dashboard never drift
/// onto different paths.
pub fn strategy_store_dir(xvn_home: &Path) -> PathBuf {
    xvn_home.join("strategies")
}

#[async_trait]
pub trait StrategyStore: Send + Sync {
    async fn save(&self, strategy: &Strategy) -> anyhow::Result<()>;
    async fn load(&self, id: &str) -> anyhow::Result<Strategy>;
    async fn list(&self) -> anyhow::Result<Vec<String>>;
    async fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// Apply a partial metadata patch to the strategy identified by
    /// `id`. `None` fields on the patch are left unchanged on disk;
    /// supplying an empty patch (every field `None`) is a valid no-op
    /// that round-trips the stored strategy unchanged.
    ///
    /// Validation runs before persistence — an invalid patch returns
    /// a [`MetadataPatchError`] (downcastable via `anyhow::Error::downcast`)
    /// without touching disk. The strategy `id` is preserved; the
    /// route layer relies on this to keep eval-run links stable.
    ///
    /// Default impl is load → validate → save so any `StrategyStore`
    /// implementation gets metadata patching for free.
    async fn update_metadata(&self, id: &str, patch: StrategyMetadataPatch) -> anyhow::Result<Strategy> {
        let mut strategy = self.load(id).await?;
        // Validation errors must surface as `MetadataPatchError` so
        // the dashboard can map them to a classified 400 instead of
        // a 500 — `anyhow::Error::from` keeps the typed cause in
        // the error chain so the caller can downcast it.
        apply_metadata_patch(&mut strategy, patch)?;
        self.save(&strategy).await?;
        Ok(strategy)
    }
}

pub struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Build the on-disk path for `id`, validating it first.
    ///
    /// All filesystem operations on this store go through here, so a
    /// rejected id is guaranteed to never reach `std::fs`. The validator
    /// rejects `..`, path separators, NUL, leading dots, and anything
    /// outside `[A-Za-z0-9_-]` — see `strategies::id` for the full set
    /// and rationale (QA finding P3-strategy-id).
    pub fn path_for(&self, id: &str) -> Result<PathBuf, StrategyIdError> {
        let id = validate_strategy_id_for_path(id)?;
        Ok(self.root.join(format!("{id}.json")))
    }
}

#[async_trait]
impl StrategyStore for FilesystemStore {
    async fn save(&self, strategy: &Strategy) -> anyhow::Result<()> {
        // F-6: single pre-persist validate seam. Any path that reaches
        // disk goes through here, so the typed-params + risk-config
        // checks run exactly once before the JSON is written. Bad
        // strategies fail with structured anyhow errors instead of
        // silently persisting and breaking the engine later.
        validate_strategy_for_persist(strategy)?;
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(&strategy.manifest.id)?;
        let json = serde_json::to_vec_pretty(strategy)?;
        tokio::fs::write(&path, json)
            .await
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    async fn load(&self, id: &str) -> anyhow::Result<Strategy> {
        let path = self.path_for(id)?;
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut ids = vec![];
        let mut rd = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(id) = name_str.strip_suffix(".json") {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let path = self.path_for(id)?;
        tokio::fs::remove_file(&path)
            .await
            .with_context(|| format!("deleting {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::manifest::PublicManifest;
    use crate::strategies::risk::RiskPreset;
    use crate::strategies::Strategy;

    fn store_in_tmp() -> (FilesystemStore, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        let store = FilesystemStore::new(td.path().to_path_buf());
        (store, td)
    }

    fn strategy_with_id(id: &str) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: id.to_string(),
                display_name: "t".into(),
                plain_summary: "t".into(),
                creator: "@tester".into(),
                template: "trend_follower".into(),
                regime_fit: vec![],
                asset_universe: vec![],
                decision_cadence_minutes: 60,
                timeframe_requirements: Default::default(),
                attested_with: vec![],
                required_tools: vec![],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
            },
            hypothesis: None,
            agents: vec![],
            pipeline: Default::default(),
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        }
    }

    #[test]
    fn creator_patch_sets_and_preserves() {
        let mut s = strategy_with_id("01HZSTRATEGYCREATOR00001A");
        assert_eq!(s.manifest.creator, "@tester");

        // Non-empty creator → set (with trimming).
        apply_metadata_patch(
            &mut s,
            StrategyMetadataPatch {
                creator: Some("  @alice  ".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.manifest.creator, "@alice");

        // None → untouched.
        apply_metadata_patch(&mut s, StrategyMetadataPatch::default()).unwrap();
        assert_eq!(s.manifest.creator, "@alice");

        // Empty/whitespace → untouched (no accidental wipe).
        apply_metadata_patch(
            &mut s,
            StrategyMetadataPatch {
                creator: Some("   ".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.manifest.creator, "@alice");
    }

    #[test]
    fn path_for_accepts_valid_id() {
        let (store, _td) = store_in_tmp();
        let p = store.path_for("01HZSTRATEGY00000000000000").unwrap();
        assert!(p.ends_with("01HZSTRATEGY00000000000000.json"));
    }

    #[test]
    fn path_for_rejects_traversal() {
        let (store, _td) = store_in_tmp();
        let err = store.path_for("../escape").unwrap_err();
        assert_eq!(err, StrategyIdError::PathSeparator);
    }

    #[test]
    fn path_for_rejects_double_dot() {
        let (store, _td) = store_in_tmp();
        let err = store.path_for("..").unwrap_err();
        assert_eq!(err, StrategyIdError::ReservedSegment);
    }

    #[test]
    fn path_for_rejects_empty() {
        let (store, _td) = store_in_tmp();
        let err = store.path_for("").unwrap_err();
        assert_eq!(err, StrategyIdError::Empty);
    }

    #[tokio::test]
    async fn load_rejected_id_does_not_touch_disk() {
        let (store, _td) = store_in_tmp();
        let err = store.load("../escape").await.unwrap_err();
        let downcast: Option<&StrategyIdError> = err.downcast_ref();
        assert!(downcast.is_some(), "expected StrategyIdError, got {err:?}");
    }

    #[tokio::test]
    async fn save_rejected_id_does_not_write_anywhere() {
        let (store, td) = store_in_tmp();
        let bad = strategy_with_id("../escape");
        let err = store.save(&bad).await.unwrap_err();
        let downcast: Option<&StrategyIdError> = err.downcast_ref();
        assert!(downcast.is_some(), "expected StrategyIdError, got {err:?}");
        // Confirm nothing was written under either the store root or its
        // parent (which traversal would have targeted).
        let mut rd = tokio::fs::read_dir(td.path()).await.unwrap();
        assert!(rd.next_entry().await.unwrap().is_none(), "store root not empty");
    }

    #[tokio::test]
    async fn delete_rejected_id_does_not_touch_disk() {
        let (store, _td) = store_in_tmp();
        let err = store.delete("../escape").await.unwrap_err();
        let downcast: Option<&StrategyIdError> = err.downcast_ref();
        assert!(downcast.is_some(), "expected StrategyIdError, got {err:?}");
    }

    #[tokio::test]
    async fn happy_path_save_load_delete_roundtrips() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGY00000000000000");
        store.save(&s).await.unwrap();
        let loaded = store.load("01HZSTRATEGY00000000000000").await.unwrap();
        assert_eq!(loaded.manifest.id, "01HZSTRATEGY00000000000000");
        store.delete("01HZSTRATEGY00000000000000").await.unwrap();
        // Loading after delete returns IO not-found, not a validation error.
        let err = store.load("01HZSTRATEGY00000000000000").await.unwrap_err();
        assert!(err.downcast_ref::<StrategyIdError>().is_none());
    }

    // ── risk round-trip regression — set-filter must not reset risk ──
    //
    // Regression: `xvn strategy set-filter` (and any load→mutate→save
    // cycle) must not reset `risk_pct_per_trade` to the Balanced preset
    // default (0.015). The existing happy-path test always uses
    // `RiskPreset::Balanced.expand()` so it would pass even if the bug
    // existed. These tests use a custom non-Balanced value (0.05) and
    // also simulate loading an old strategy file that predates the
    // `max_position_pct_nav` field (added 2026-06-03) to ensure the
    // serde default for that field does not disturb other risk fields.

    #[tokio::test]
    async fn custom_risk_pct_per_trade_survives_round_trip() {
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYRISK0000000001");
        s.risk.risk_pct_per_trade = 0.05;
        store.save(&s).await.unwrap();

        let loaded = store.load("01HZSTRATEGYRISK0000000001").await.unwrap();
        assert_eq!(
            loaded.risk.risk_pct_per_trade, 0.05,
            "risk_pct_per_trade must not be reset to Balanced default (0.015) after save/load"
        );
    }

    #[tokio::test]
    async fn risk_pct_preserved_through_load_mutate_save_cycle() {
        // Simulates what `xvn strategy set-filter` does: load → change
        // only activation_mode/filter → save. The risk block must be
        // carried through untouched.
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYRISK0000000002");
        s.risk.risk_pct_per_trade = 0.05;
        store.save(&s).await.unwrap();

        let mut loaded = store.load("01HZSTRATEGYRISK0000000002").await.unwrap();
        // Simulate the mutations set_filter makes: it flips activation_mode to
        // FilterGated AND attaches a filter (a FilterGated strategy with no
        // filter fails the load invariant, so both must change together).
        loaded.activation_mode = xvision_filters::ActivationMode::FilterGated;
        loaded.filter = Some(
            serde_json::from_value(serde_json::json!({
                "id": "f_01JX0000000000000000000000",
                "strategy_id": "01HZSTRATEGYRISK0000000002",
                "display_name": "test filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": { "all": [] }
            }))
            .expect("minimal filter must parse"),
        );
        store.save(&loaded).await.unwrap();

        let after = store.load("01HZSTRATEGYRISK0000000002").await.unwrap();
        assert_eq!(
            after.risk.risk_pct_per_trade, 0.05,
            "risk_pct_per_trade must survive a set-filter-style load/mutate/save cycle"
        );
    }

    #[tokio::test]
    async fn old_json_without_max_position_pct_nav_preserves_risk_pct() {
        // Strategy JSON written before `max_position_pct_nav` was added
        // (2026-06-03 commit). The serde default fills 20.0 for the new
        // field without touching `risk_pct_per_trade`.
        let (store, td) = store_in_tmp();
        let id = "01HZSTRATEGYRISK0000000003";
        let json = serde_json::json!({
            "manifest": {
                "id": id,
                "display_name": "old",
                "plain_summary": "pre-max_position_pct_nav",
                "creator": "@tester",
                "template": "trend_follower",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "attested_with": [],
                "required_tools": [],
                "risk_preset_or_config": "custom"
            },
            "risk": {
                "risk_pct_per_trade": 0.05,
                "max_concurrent_positions": 2,
                "max_leverage": 3.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
                // max_position_pct_nav intentionally absent
            }
        });
        let path = td.path().join(format!("{id}.json"));
        tokio::fs::write(&path, serde_json::to_vec_pretty(&json).unwrap())
            .await
            .unwrap();

        let loaded = store.load(id).await.unwrap();
        assert_eq!(
            loaded.risk.risk_pct_per_trade, 0.05,
            "risk_pct_per_trade must be read from old JSON, not reset by serde default"
        );
        assert_eq!(
            loaded.risk.max_position_pct_nav, 20.0,
            "max_position_pct_nav must be filled with serde default (20.0) when absent from old JSON"
        );
    }

    // ── strategy-edit-top-level-fields — metadata patch ─────────────

    #[tokio::test]
    async fn update_metadata_applies_provided_fields_and_leaves_others_alone() {
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYMETA0000000001");
        s.manifest.display_name = "Old title".into();
        s.manifest.plain_summary = "Old summary".into();
        s.manifest.asset_universe = vec!["BTC/USD".into()];
        s.manifest.template = "trend_follower".into();
        // template is meaningful — it must not be mutated by the patch.
        store.save(&s).await.unwrap();

        let patched = store
            .update_metadata(
                "01HZSTRATEGYMETA0000000001",
                StrategyMetadataPatch {
                    display_name: Some("New title".into()),
                    plain_summary: None,
                    asset_universe: Some(vec!["eth/usd".into(), "btc/usd".into()]),
                    decision_cadence_minutes: Some(240),
                    color: None,
                    creator: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(patched.manifest.id, "01HZSTRATEGYMETA0000000001");
        assert_eq!(patched.manifest.display_name, "New title");
        // Untouched field stays put.
        assert_eq!(patched.manifest.plain_summary, "Old summary");
        // Assets are normalized + de-duped.
        assert_eq!(patched.manifest.asset_universe, vec!["ETH/USD", "BTC/USD"]);
        assert_eq!(patched.manifest.decision_cadence_minutes, 240);
        // Out-of-scope fields untouched.
        assert_eq!(patched.manifest.template, "trend_follower");
        assert!(patched.manifest.published_at.is_none());
    }

    #[tokio::test]
    async fn update_metadata_empty_patch_is_valid_noop() {
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYNOOP0000000001");
        s.manifest.display_name = "Stable".into();
        s.manifest.plain_summary = "Stable summary".into();
        s.manifest.asset_universe = vec!["BTC/USD".into()];
        store.save(&s).await.unwrap();

        let after = store
            .update_metadata("01HZSTRATEGYNOOP0000000001", StrategyMetadataPatch::default())
            .await
            .unwrap();

        assert_eq!(after.manifest.display_name, "Stable");
        assert_eq!(after.manifest.plain_summary, "Stable summary");
        assert_eq!(after.manifest.asset_universe, vec!["BTC/USD"]);
    }

    #[tokio::test]
    async fn update_metadata_rejects_empty_display_name_without_touching_disk() {
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYBADNAME000000A");
        s.manifest.display_name = "Original".into();
        store.save(&s).await.unwrap();

        let err = store
            .update_metadata(
                "01HZSTRATEGYBADNAME000000A",
                StrategyMetadataPatch {
                    display_name: Some("   ".into()),
                    plain_summary: None,
                    asset_universe: None,
                    decision_cadence_minutes: None,
                    color: None,
                    creator: None,
                },
            )
            .await
            .expect_err("blank display_name must be rejected");
        // Typed error is downcastable for the dashboard classifier.
        let typed: Option<&MetadataPatchError> = err.downcast_ref();
        assert_eq!(typed, Some(&MetadataPatchError::EmptyDisplayName));

        // Disk is untouched — display_name stayed as "Original".
        let loaded = store.load("01HZSTRATEGYBADNAME000000A").await.unwrap();
        assert_eq!(loaded.manifest.display_name, "Original");
    }

    #[tokio::test]
    async fn update_metadata_rejects_blank_asset_entry() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGYBLANKASSET0001");
        store.save(&s).await.unwrap();

        let err = store
            .update_metadata(
                "01HZSTRATEGYBLANKASSET0001",
                StrategyMetadataPatch {
                    asset_universe: Some(vec!["BTC/USD".into(), "  ".into()]),
                    ..Default::default()
                },
            )
            .await
            .expect_err("blank asset entry must be rejected");
        assert_eq!(
            err.downcast_ref::<MetadataPatchError>(),
            Some(&MetadataPatchError::BlankAssetEntry)
        );
    }

    #[tokio::test]
    async fn update_metadata_rejects_invalid_asset_symbol() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGYBADASSET00001Z");
        store.save(&s).await.unwrap();

        let err = store
            .update_metadata(
                "01HZSTRATEGYBADASSET00001Z",
                StrategyMetadataPatch {
                    asset_universe: Some(vec!["not-an-asset".into()]),
                    ..Default::default()
                },
            )
            .await
            .expect_err("symbol without `/` must be rejected");
        match err.downcast_ref::<MetadataPatchError>() {
            Some(MetadataPatchError::InvalidAssetSymbol(_)) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_metadata_rejects_empty_asset_list() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGYEMPTYASSETS001");
        store.save(&s).await.unwrap();

        let err = store
            .update_metadata(
                "01HZSTRATEGYEMPTYASSETS001",
                StrategyMetadataPatch {
                    asset_universe: Some(vec![]),
                    ..Default::default()
                },
            )
            .await
            .expect_err("empty asset list must be rejected when provided");
        assert_eq!(
            err.downcast_ref::<MetadataPatchError>(),
            Some(&MetadataPatchError::EmptyAssetUniverse)
        );
    }

    #[tokio::test]
    async fn update_metadata_preserves_strategy_id_across_edit() {
        // The cycle-id-stable invariant: editing top-level fields must
        // never change `manifest.id`. Eval runs reference the strategy
        // by this id, so a rename would orphan them.
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGYIDSTABLE00001A");
        store.save(&s).await.unwrap();

        let patched = store
            .update_metadata(
                "01HZSTRATEGYIDSTABLE00001A",
                StrategyMetadataPatch {
                    display_name: Some("Renamed".into()),
                    plain_summary: Some("New summary".into()),
                    asset_universe: Some(vec!["ETH/USD".into()]),
                    decision_cadence_minutes: None,
                    color: None,
                    creator: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(patched.manifest.id, "01HZSTRATEGYIDSTABLE00001A");
        let reloaded = store.load("01HZSTRATEGYIDSTABLE00001A").await.unwrap();
        assert_eq!(reloaded.manifest.id, "01HZSTRATEGYIDSTABLE00001A");
        assert_eq!(reloaded.manifest.display_name, "Renamed");
    }

    // ── color patch tests ────────────────────────────────────────────

    #[tokio::test]
    async fn set_color_then_reload_equals() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGYCOLOR00000001A");
        store.save(&s).await.unwrap();

        let patched = store
            .update_metadata(
                "01HZSTRATEGYCOLOR00000001A",
                StrategyMetadataPatch {
                    color: Some("#D4A547".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(patched.manifest.color, Some("#D4A547".into()));
        let reloaded = store.load("01HZSTRATEGYCOLOR00000001A").await.unwrap();
        assert_eq!(reloaded.manifest.color, Some("#D4A547".into()));
    }

    #[tokio::test]
    async fn clear_color_via_empty_string_becomes_none() {
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYCOLOR00000002B");
        s.manifest.color = Some("#6BAFA8".into());
        store.save(&s).await.unwrap();

        let patched = store
            .update_metadata(
                "01HZSTRATEGYCOLOR00000002B",
                StrategyMetadataPatch {
                    color: Some("".into()), // explicit clear signal
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(patched.manifest.color, None);
        let reloaded = store.load("01HZSTRATEGYCOLOR00000002B").await.unwrap();
        assert_eq!(reloaded.manifest.color, None);
    }

    #[tokio::test]
    async fn invalid_hex_color_returns_error() {
        let (store, _td) = store_in_tmp();
        let s = strategy_with_id("01HZSTRATEGYCOLOR00000003C");
        store.save(&s).await.unwrap();

        for bad in &["#ZZZ", "D4A547", "#D4A54", "#D4A54777", "red", "#"] {
            let err = store
                .update_metadata(
                    "01HZSTRATEGYCOLOR00000003C",
                    StrategyMetadataPatch {
                        color: Some((*bad).into()),
                        ..Default::default()
                    },
                )
                .await
                .expect_err("invalid hex color must be rejected");
            match err.downcast_ref::<MetadataPatchError>() {
                Some(MetadataPatchError::InvalidColor(_)) => {}
                other => panic!("expected InvalidColor for {bad:?}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn color_none_leaves_existing_color_unchanged() {
        let (store, _td) = store_in_tmp();
        let mut s = strategy_with_id("01HZSTRATEGYCOLOR00000004D");
        s.manifest.color = Some("#E07A3A".into());
        store.save(&s).await.unwrap();

        // Patch without color field — should leave it alone.
        let patched = store
            .update_metadata(
                "01HZSTRATEGYCOLOR00000004D",
                StrategyMetadataPatch {
                    display_name: Some("New name".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(patched.manifest.color, Some("#E07A3A".into()));
    }
}
