//! `ProviderRegistry` — resolves `SlotRef` to backend `Arc`s, memoizing one
//! instance per `(provider, model)` so two arms sharing a slot share an HTTP
//! client. See spec §3.3 of
//! `docs/superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::slot::SlotRef;
use xvision_intern::backend::{AnthropicIntern, InternBackend, OpenAICompatIntern};
use xvision_trader::{OpenAiCompatBackend, TraderBackend};

pub struct ProviderRegistry {
    rows: Vec<ProviderEntry>,
    pub default_intern: SlotRef,
    pub default_trader: SlotRef,
    intern_cache: Mutex<HashMap<(String, String), Arc<dyn InternBackend>>>,
    trader_cache: Mutex<HashMap<(String, String), Arc<dyn TraderBackend>>>,
}

impl ProviderRegistry {
    pub fn new(rows: Vec<ProviderEntry>, default_intern: SlotRef, default_trader: SlotRef) -> Self {
        Self {
            rows,
            default_intern,
            default_trader,
            intern_cache: Mutex::new(HashMap::new()),
            trader_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn rows(&self) -> &[ProviderEntry] {
        &self.rows
    }

    /// Resolve an intern slot to a backend `Arc`, filling in the provider
    /// from `default_intern.provider` when the slot's provider segment is
    /// empty (the shorthand-form marker emitted by `parse_arm_spec`).
    /// Memoized on `(provider, model)` — two arms requesting the same slot
    /// share one HTTP client.
    pub fn intern_backend(&self, slot: &SlotRef) -> Result<Arc<dyn InternBackend>> {
        let resolved = self.fill_default_provider(slot, &self.default_intern);
        let key = (resolved.provider.clone(), resolved.model.clone());
        {
            let cache = self.intern_cache.lock().unwrap();
            if let Some(b) = cache.get(&key) {
                return Ok(Arc::clone(b));
            }
        }
        let row = self.find_provider(&resolved.provider, "intern")?;
        let backend: Arc<dyn InternBackend> = match row.kind {
            ProviderKind::Anthropic => Arc::new(AnthropicIntern::from_env(
                row.base_url.clone(),
                &resolved.model,
                &row.api_key_env,
            )?),
            ProviderKind::OpenaiCompat => Arc::new(OpenAICompatIntern::from_env(
                row.base_url.clone(),
                &resolved.model,
                &row.api_key_env,
            )?),
            ProviderKind::LocalCandle => {
                return Err(anyhow!(
                    "provider `{}` kind=local-candle is not yet supported as an Intern slot",
                    resolved.provider
                ));
            }
        };
        self.intern_cache
            .lock()
            .unwrap()
            .insert(key, Arc::clone(&backend));
        Ok(backend)
    }

    /// Trader analogue of `intern_backend`. Anthropic is not yet wired as a
    /// trader provider — only OpenAI-compat (which covers OpenAI, Ollama,
    /// Together, OpenRouter, etc.) ships in this Phase. Adding Anthropic as
    /// a trader is a follow-up plan.
    pub fn trader_backend(&self, slot: &SlotRef) -> Result<Arc<dyn TraderBackend>> {
        let resolved = self.fill_default_provider(slot, &self.default_trader);
        let key = (resolved.provider.clone(), resolved.model.clone());
        {
            let cache = self.trader_cache.lock().unwrap();
            if let Some(b) = cache.get(&key) {
                return Ok(Arc::clone(b));
            }
        }
        let row = self.find_provider(&resolved.provider, "trader")?;
        let backend: Arc<dyn TraderBackend> = match row.kind {
            ProviderKind::OpenaiCompat => Arc::new(OpenAiCompatBackend::from_env(
                row.base_url.clone(),
                &resolved.model,
                &row.api_key_env,
            )?),
            ProviderKind::Anthropic => {
                return Err(anyhow!(
                    "provider `{}` kind=anthropic is not yet supported as a Trader slot \
                     (only openai-compat trader backends are wired in this phase)",
                    resolved.provider
                ));
            }
            ProviderKind::LocalCandle => {
                return Err(anyhow!(
                    "provider `{}` kind=local-candle is not yet supported as a Trader slot",
                    resolved.provider
                ));
            }
        };
        self.trader_cache
            .lock()
            .unwrap()
            .insert(key, Arc::clone(&backend));
        Ok(backend)
    }

    fn fill_default_provider(&self, slot: &SlotRef, default: &SlotRef) -> SlotRef {
        if slot.provider.is_empty() {
            SlotRef::new(default.provider.clone(), slot.model.clone())
        } else {
            slot.clone()
        }
    }

    fn find_provider(&self, name: &str, role: &str) -> Result<&ProviderEntry> {
        self.rows.iter().find(|p| p.name == name).ok_or_else(|| {
            let known: Vec<&str> = self.rows.iter().map(|p| p.name.as_str()).collect();
            anyhow!(
                "provider `{name}` referenced by {role} slot not registered.\n\
                 known providers: {}\n\
                 add it to config/default.toml under [[providers]].",
                known.join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(rows: Vec<ProviderEntry>) -> ProviderRegistry {
        ProviderRegistry::new(
            rows,
            SlotRef::new("anthropic", "claude-haiku-4-5"),
            SlotRef::new("openai", "gpt-4o"),
        )
    }

    fn err_msg<T>(r: Result<T>) -> String {
        match r {
            Ok(_) => panic!("expected Err, got Ok"),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn missing_provider_yields_actionable_error() {
        let reg = registry_with(vec![]);
        let msg = err_msg(reg.intern_backend(&SlotRef::new("nope", "x")));
        assert!(msg.contains("nope"), "actual: {msg}");
        assert!(msg.contains("known providers"), "actual: {msg}");
    }

    #[test]
    fn unknown_kind_local_candle_errors_for_intern() {
        let row = ProviderEntry {
            name: "local".into(),
            kind: ProviderKind::LocalCandle,
            base_url: "models/x.gguf".into(),
            api_key_env: "".into(),
            enabled_models: Vec::new(),
        };
        let reg = registry_with(vec![row]);
        let msg = err_msg(reg.intern_backend(&SlotRef::new("local", "x")));
        assert!(msg.contains("local-candle"), "actual: {msg}");
    }

    #[test]
    fn intern_backend_memoizes_on_provider_model() {
        std::env::set_var("DUMMY_KEY", "k");
        let row = ProviderEntry {
            name: "openai".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "DUMMY_KEY".into(),
            enabled_models: Vec::new(),
        };
        let reg = ProviderRegistry::new(
            vec![row],
            SlotRef::new("openai", "gpt-4o"),
            SlotRef::new("openai", "gpt-4o"),
        );
        let a = reg.intern_backend(&SlotRef::new("openai", "gpt-4o")).unwrap();
        let b = reg.intern_backend(&SlotRef::new("openai", "gpt-4o")).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "same slot must yield the same Arc");
        let c = reg
            .intern_backend(&SlotRef::new("openai", "gpt-4o-mini"))
            .unwrap();
        assert!(!Arc::ptr_eq(&a, &c), "different model must yield a different Arc");
    }

    #[test]
    fn trader_backend_memoizes_on_provider_model() {
        std::env::set_var("DUMMY_KEY", "k");
        let row = ProviderEntry {
            name: "openai".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "DUMMY_KEY".into(),
            enabled_models: Vec::new(),
        };
        let reg = ProviderRegistry::new(
            vec![row],
            SlotRef::new("openai", "gpt-4o"),
            SlotRef::new("openai", "gpt-4o"),
        );
        let a = reg.trader_backend(&SlotRef::new("openai", "gpt-4o")).unwrap();
        let b = reg.trader_backend(&SlotRef::new("openai", "gpt-4o")).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "same slot must yield the same Arc");
        let c = reg
            .trader_backend(&SlotRef::new("openai", "gpt-4o-mini"))
            .unwrap();
        assert!(!Arc::ptr_eq(&a, &c), "different model must yield a different Arc");
    }

    #[test]
    fn shorthand_provider_falls_back_to_default() {
        std::env::set_var("DUMMY_KEY", "k");
        let row = ProviderEntry {
            name: "openai".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "DUMMY_KEY".into(),
            enabled_models: Vec::new(),
        };
        let reg = ProviderRegistry::new(
            vec![row],
            SlotRef::new("openai", "gpt-4o"),
            SlotRef::new("openai", "gpt-4o"),
        );
        // Shorthand form: empty provider, model only — registry should fill
        // in the default intern provider.
        let shorthand = SlotRef::new("", "gpt-4o-mini");
        let resolved = reg.intern_backend(&shorthand).unwrap();
        let explicit = reg
            .intern_backend(&SlotRef::new("openai", "gpt-4o-mini"))
            .unwrap();
        assert!(
            Arc::ptr_eq(&resolved, &explicit),
            "shorthand must resolve to the same Arc as the explicit form"
        );
    }

    #[test]
    fn missing_provider_for_trader_yields_actionable_error() {
        let reg = registry_with(vec![]);
        let msg = err_msg(reg.trader_backend(&SlotRef::new("nope", "x")));
        assert!(msg.contains("nope"), "actual: {msg}");
        assert!(msg.contains("trader"), "actual: {msg}");
    }
}
