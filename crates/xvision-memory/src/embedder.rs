//! Embedder trait + adapters.
//!
//! The OpenAI adapter is implemented in `xvision-engine` (where the
//! provider client already lives). This module only defines the
//! abstract trait + a `StaticEmbedder` used by tests and by the
//! disabled-by-default unit-test path.

use async_trait::async_trait;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    fn id(&self) -> &str;
    fn dim(&self) -> usize;
}

pub struct StaticEmbedder {
    id: String,
    vector: Vec<f32>,
}

impl StaticEmbedder {
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self { id: id.into(), vector }
    }
}

#[async_trait]
impl Embedder for StaticEmbedder {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.vector.clone())
    }
    fn id(&self) -> &str { &self.id }
    fn dim(&self) -> usize { self.vector.len() }
}
