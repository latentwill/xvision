//! Strategy authoring dispatcher — pure Rust functions over `&dyn BundleStore`
//! that mutate `StrategyBundle`s. Both surfaces call into here:
//!
//! - `xvision-mcp` exposes these to external AI agents via MCP tool calls
//!   (`xvn_create_strategy`, `xvn_update_slot`, ...).
//! - `xvision-dashboard::wizard_loop` drives the same verbs from the
//!   server-side wizard agent over the tool-use loop.
//!
//! Errors are flat `anyhow::Result`; surface-specific error mapping
//! (rmcp::ErrorData, axum::Json, etc.) happens at the call site.

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::bundle::{
    risk::{RiskConfig, RiskPreset},
    slot::LLMSlot,
    store::BundleStore,
    validate::validate_bundle,
    StrategyBundle,
};
use crate::templates::registry as template_registry;

// ---------------------------------------------------------------------------
// types — request / response shapes shared by both surfaces.
// MCP wraps these with JsonSchema derives in its own request structs; the
// dashboard speaks them directly.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub display_name: String,
    pub plain_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStrategyReq {
    pub template: String,
    pub name: String,
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStrategyOut {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSlotReq {
    pub id: String,
    pub slot: String,
    pub prompt: Option<String>,
    pub model_requirement: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSlotOut {
    pub id: String,
    pub updated: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMechanicalParamReq {
    pub id: String,
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRiskConfigReq {
    pub id: String,
    pub preset: Option<String>,
    pub explicit: Option<RiskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRiskConfigOut {
    pub id: String,
    /// `preset` or `explicit`.
    pub applied: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateDraftOut {
    pub id: String,
    pub ok: bool,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// dispatcher functions
// ---------------------------------------------------------------------------

pub fn list_templates() -> Vec<TemplateInfo> {
    template_registry::list_template_names()
        .iter()
        .filter_map(|name| {
            template_registry::get(name).map(|t| TemplateInfo {
                name: t.name().to_string(),
                display_name: t.display_name().to_string(),
                plain_summary: t.plain_summary().to_string(),
            })
        })
        .collect()
}

pub async fn create_strategy(
    store: &dyn BundleStore,
    req: CreateStrategyReq,
) -> anyhow::Result<CreateStrategyOut> {
    let tpl = template_registry::get(&req.template).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown template '{}' — try list_templates",
            req.template
        )
    })?;
    let id = Ulid::new().to_string();
    let creator = req.creator.unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), req.name, creator);
    store.save(&draft).await?;
    Ok(CreateStrategyOut { id })
}

pub async fn get_strategy(
    store: &dyn BundleStore,
    id: &str,
) -> anyhow::Result<StrategyBundle> {
    store.load(id).await
}

pub async fn update_slot(
    store: &dyn BundleStore,
    req: UpdateSlotReq,
) -> anyhow::Result<UpdateSlotOut> {
    let mut bundle = store.load(&req.id).await?;
    let slot_field = match req.slot.as_str() {
        "regime" => &mut bundle.regime_slot,
        "intern" => &mut bundle.intern_slot,
        "trader" => &mut bundle.trader_slot,
        other => anyhow::bail!(
            "unknown slot `{other}` — must be one of: regime, intern, trader"
        ),
    };
    let slot = slot_field.get_or_insert_with(|| LLMSlot {
        role: req.slot.clone(),
        prompt: String::new(),
        model_requirement: String::new(),
        allowed_tools: vec![],
    });
    let mut updated: Vec<String> = Vec::new();
    if let Some(p) = req.prompt {
        slot.prompt = p;
        updated.push("prompt".into());
    }
    if let Some(m) = req.model_requirement {
        slot.model_requirement = m;
        updated.push("model_requirement".into());
    }
    if let Some(t) = req.allowed_tools {
        slot.allowed_tools = t;
        updated.push("allowed_tools".into());
    }
    if updated.is_empty() {
        anyhow::bail!(
            "no fields to update — supply at least one of prompt / model_requirement / allowed_tools"
        );
    }
    store.save(&bundle).await?;
    Ok(UpdateSlotOut {
        id: req.id,
        updated,
    })
}

pub async fn set_mechanical_param(
    store: &dyn BundleStore,
    req: SetMechanicalParamReq,
) -> anyhow::Result<()> {
    let mut bundle = store.load(&req.id).await?;
    let map = bundle.mechanical_params.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "mechanical_params is not a JSON object — template invariant violation"
        )
    })?;
    map.insert(req.key, req.value);
    store.save(&bundle).await
}

pub async fn set_risk_config(
    store: &dyn BundleStore,
    req: SetRiskConfigReq,
) -> anyhow::Result<SetRiskConfigOut> {
    let (config, applied) = match (req.preset, req.explicit) {
        (Some(p), None) => {
            let preset = match p.as_str() {
                "conservative" => RiskPreset::Conservative,
                "balanced" => RiskPreset::Balanced,
                "aggressive" => RiskPreset::Aggressive,
                other => anyhow::bail!(
                    "unknown preset `{other}` — must be one of: conservative, balanced, aggressive"
                ),
            };
            (preset.expand(), "preset")
        }
        (None, Some(cfg)) => (cfg, "explicit"),
        (Some(_), Some(_)) => anyhow::bail!("preset and explicit are mutually exclusive"),
        (None, None) => anyhow::bail!("supply either preset or explicit"),
    };
    let mut bundle = store.load(&req.id).await?;
    bundle.risk = config;
    store.save(&bundle).await?;
    Ok(SetRiskConfigOut {
        id: req.id,
        applied: applied.into(),
    })
}

pub async fn validate_draft(
    store: &dyn BundleStore,
    id: &str,
) -> anyhow::Result<ValidateDraftOut> {
    let bundle = store.load(id).await?;
    let (ok, errors) = match validate_bundle(&bundle) {
        Ok(()) => (true, vec![]),
        Err(e) => (false, vec![e.to_string()]),
    };
    Ok(ValidateDraftOut {
        id: id.to_string(),
        ok,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::store::FilesystemStore;

    fn store_in_tmp() -> (FilesystemStore, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        let store = FilesystemStore::new(td.path().to_path_buf());
        (store, td)
    }

    #[test]
    fn list_templates_returns_known_set() {
        let names: Vec<_> = list_templates().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"trend_follower".to_string()));
        assert!(names.contains(&"breakout".to_string()));
        assert!(names.contains(&"mean_reversion".to_string()));
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "btc-mom-1".into(),
                creator: Some("@test".into()),
            },
        )
        .await
        .unwrap();
        let bundle = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(bundle.manifest.id, out.id);
        assert_eq!(bundle.manifest.template, "trend_follower");
    }

    #[tokio::test]
    async fn update_slot_reports_updated_fields() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let upd = update_slot(
            &store,
            UpdateSlotReq {
                id: out.id.clone(),
                slot: "trader".into(),
                prompt: Some("New prompt".into()),
                model_requirement: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(upd.updated, vec!["prompt".to_string()]);

        let bundle = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(bundle.trader_slot.unwrap().prompt, "New prompt");
    }

    #[tokio::test]
    async fn set_risk_config_preset_balanced() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let r = set_risk_config(
            &store,
            SetRiskConfigReq {
                id: out.id.clone(),
                preset: Some("balanced".into()),
                explicit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(r.applied, "preset");

        let bundle = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(bundle.risk.risk_pct_per_trade, 0.015);
    }

    #[tokio::test]
    async fn validate_draft_passes_for_fresh_template() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let v = validate_draft(&store, &out.id).await.unwrap();
        assert!(v.ok);
        assert!(v.errors.is_empty());
    }
}
