//! Model adapter (Phase 3.3).
//!
//! Bridges `dspy-rs`'s LM concept to an xvision-facing trait. Per the dependency
//! spike, the workspace has **no** rig-core of its own — rig-core enters only
//! transitively through dspy-rs — so this adapter does not match an existing
//! workspace integration; it defines the boundary fresh.
//!
//! Two implementations:
//! * [`DeterministicTestModel`] — `DummyLM`-backed, in-memory, no network. Used
//!   for CI and reproducible optimization runs.
//! * [`live`] — a feature-gated stub for a real provider-backed model. It does
//!   **not** call the network in this crate (see its `// TODO live`).
//!
//! All calls thread [`Provenance`] for provider/model identity + token/cost
//! accounting.

use std::sync::{Arc, Mutex};

use dspy_rs::{Chat, DummyLM, Example, LmUsage};

use crate::error::{OptimizerError, OptimizerResult};

/// Provider/model identity + token & cost accounting recorded for every model
/// call. Aggregated into an [`OptimizationSnapshot`](crate::snapshot) so a run is
/// fully attributable.
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    /// Provider key, e.g. `dummy`, `openai`, `anthropic`.
    pub provider: String,
    /// Model identifier as the provider names it, e.g. `gpt-4o-mini`.
    pub model: String,
    /// Total prompt tokens billed across calls.
    pub prompt_tokens: u64,
    /// Total completion tokens billed across calls.
    pub completion_tokens: u64,
    /// Estimated cost in USD micros (1e-6 USD) accumulated across calls. Kept as
    /// an integer to stay deterministic / hashable; `0` when cost is unknown
    /// (e.g. the deterministic test model is free).
    pub cost_micros_usd: u64,
}

impl Provenance {
    /// Fresh provenance for a `provider`/`model` with zeroed accounting.
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micros_usd: 0,
        }
    }

    /// Fold a single call's [`LmUsage`] into the running totals.
    pub fn record_usage(&mut self, usage: &LmUsage) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(usage.prompt_tokens);
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(usage.completion_tokens);
    }

    /// Total tokens across prompt + completion.
    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens.saturating_add(self.completion_tokens)
    }
}

/// One model completion: the assistant text plus the provenance snapshot at the
/// time of the call.
#[derive(Clone, Debug)]
pub struct ModelCompletion {
    /// Raw assistant text. Parsed/validated by the signature boundary.
    pub text: String,
    /// Provenance accumulated up to and including this call.
    pub provenance: Provenance,
}

/// xvision-facing model abstraction the optimizer drives. Intentionally narrow:
/// the optimizer only needs to turn a rendered prompt into completion text and
/// accumulate provenance. No live decision-cycle dispatch is exposed.
#[async_trait::async_trait]
pub trait OptimizerModel: Send + Sync {
    /// Run a single completion over `chat`. `seed_example` is the DSRs
    /// [`Example`] context the call is attached to (used by the deterministic
    /// model's cache + by real backends to thread structured inputs).
    async fn complete(
        &self,
        seed_example: Example,
        chat: Chat,
    ) -> OptimizerResult<ModelCompletion>;

    /// Current accumulated provenance (provider/model + token/cost totals).
    fn provenance(&self) -> Provenance;
}

/// Deterministic, in-memory, **no-network** model. Backed by `dspy-rs`'s
/// `DummyLM`. The `scripted` response is returned verbatim for every call, which
/// is exactly what we want for reproducible optimization and CI: identical inputs
/// always yield identical outputs.
pub struct DeterministicTestModel {
    inner: DummyLM,
    /// The canned assistant text returned for every `complete` call.
    scripted: String,
    provenance: Arc<Mutex<Provenance>>,
}

impl DeterministicTestModel {
    /// Construct a deterministic model that always replies with `scripted`.
    /// Provider/model identity is recorded as `dummy`/`dummy` in provenance.
    pub async fn new(scripted: impl Into<String>) -> Self {
        Self {
            inner: DummyLM::new().await,
            scripted: scripted.into(),
            provenance: Arc::new(Mutex::new(Provenance::new("dummy", "dummy"))),
        }
    }

    /// Run a completion. Free function form so callers that don't want the trait
    /// object can use it directly; the trait impl delegates here.
    pub async fn complete_inner(
        &self,
        seed_example: Example,
        chat: Chat,
    ) -> OptimizerResult<ModelCompletion> {
        let response = self
            .inner
            .call(seed_example, chat, self.scripted.clone())
            .await
            .map_err(|e| OptimizerError::Engine(e.to_string()))?;

        let mut prov = self.provenance.lock().expect("provenance mutex poisoned");
        prov.record_usage(&response.usage);
        // DummyLM is free: cost stays 0.
        let snapshot = prov.clone();
        drop(prov);

        Ok(ModelCompletion {
            text: response.output.content(),
            provenance: snapshot,
        })
    }

    fn provenance_snapshot(&self) -> Provenance {
        self.provenance
            .lock()
            .expect("provenance mutex poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl OptimizerModel for DeterministicTestModel {
    async fn complete(
        &self,
        seed_example: Example,
        chat: Chat,
    ) -> OptimizerResult<ModelCompletion> {
        self.complete_inner(seed_example, chat).await
    }

    fn provenance(&self) -> Provenance {
        self.provenance_snapshot()
    }
}

/// Feature-gated live model backend. NOT compiled by default and NOT wired to the
/// network here — promoting it to a real backend is a deliberate, separate task.
#[cfg(feature = "live")]
pub mod live {
    use super::*;

    /// Real provider-backed model. **Stub.** Constructing one or calling it
    /// always returns [`OptimizerError::ProviderUnavailable`] until the live path
    /// is implemented — this preserves the crate's offline-only invariant even
    /// when the `live` feature is on.
    pub struct LiveModel {
        provenance: Provenance,
    }

    impl LiveModel {
        /// Build a live model handle for `provider`/`model`.
        ///
        /// TODO live: bridge to xvision's dispatch (Cline sidecar / LlmDispatch)
        /// by implementing a rig-core completion model that dspy-rs's `LM` can
        /// consume. Must remain OFFLINE w.r.t. the trading decision cycle.
        pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
            Self {
                provenance: Provenance::new(provider, model),
            }
        }

        /// TODO live: perform a real completion. Currently a hard stub.
        pub async fn complete_inner(
            &self,
            _seed_example: Example,
            _chat: Chat,
        ) -> OptimizerResult<ModelCompletion> {
            Err(OptimizerError::ProviderUnavailable {
                provider: self.provenance.provider.clone(),
                detail: "live provider backend is a stub in xvision-dspy; enable \
                         and implement the rig-core bridge (TODO live) before use"
                    .to_string(),
            })
        }
    }
}
