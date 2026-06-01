use async_trait::async_trait;

/// Offline DSPy compilation bridge. The engine never depends on xvision-dspy
/// directly; callers that wire up the flywheel supply a concrete implementation.
/// Tests and disabled paths use `NullDspyBridge`.
#[async_trait]
pub trait DspyBridge: Send + Sync {
    /// Compile a DSR (Demonstrate-Search-Retrieve) instruction from a cohort
    /// of observation texts. Returns the compiled instruction string to be
    /// persisted as a Pattern and injected into future mutator prompts.
    async fn compile(
        &self,
        namespace: &str,
        observation_texts: &[String],
    ) -> anyhow::Result<String>;
}

/// No-op bridge used when `dspy_enabled = false` or in tests that don't need
/// the compile path.
pub struct NullDspyBridge;

#[async_trait]
impl DspyBridge for NullDspyBridge {
    async fn compile(
        &self,
        _namespace: &str,
        _observation_texts: &[String],
    ) -> anyhow::Result<String> {
        Ok(String::new())
    }
}
