//! Thin OpenAI embeddings adapter for V2D memory.
//!
//! Calls `POST {base_url}/embeddings` against the operator's configured
//! OpenAI-compat endpoint with model `text-embedding-3-small` (1536-dim)
//! by default. Returns the single embedding vector or an error. The
//! `Embedder` trait impl is intentionally narrow; the existing
//! provider dispatch client is not reused because the request/response
//! shape is different and the coupling cost outweighs the share.
//!
//! Construction takes the base URL + API key explicitly so the engine
//! startup wiring can pull the operator's configured OpenAI provider
//! (or a compatible alternative) without this module having to know
//! about the providers crate.

use async_trait::async_trait;
use serde::Deserialize;
use xvision_memory::embedder::Embedder;

/// Default OpenAI embedding model. Picked to match the dimensionality
/// (1536) recorded by the memory store schema in migration
/// `xvision-memory/migrations/...` so a populated DB stays roundtrip-
/// compatible if an operator swaps embedders.
const DEFAULT_MODEL: &str = "text-embedding-3-small";
const DEFAULT_DIM: usize = 1536;

pub struct OpenAiEmbedder {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiEmbedder {
    /// Build an embedder pointing at `base_url` (e.g.
    /// `https://api.openai.com/v1`) with the given API key. The model
    /// defaults to `text-embedding-3-small`; override via
    /// `with_model`.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Override the embedding model. The reported `dim()` stays
    /// `DEFAULT_DIM` because we don't have a model→dim table in this
    /// crate; production callers should align migrations with the
    /// model they pick.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingEntry>,
}

#[derive(Deserialize)]
struct EmbeddingEntry {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": &self.model,
            "input": text,
        });
        let resp: EmbeddingsResponse = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp
            .data
            .into_iter()
            .next()
            .map(|e| e.embedding)
            .unwrap_or_default())
    }

    fn id(&self) -> &str {
        "openai:text-embedding-3-small"
    }

    fn dim(&self) -> usize {
        DEFAULT_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedder_reports_canonical_id_and_dim() {
        let e = OpenAiEmbedder::new("https://api.openai.com/v1", "test-key");
        assert_eq!(e.id(), "openai:text-embedding-3-small");
        assert_eq!(e.dim(), 1536);
    }

    #[test]
    fn with_model_overrides_model_but_keeps_id_stable() {
        // The id() string is fixed to the default model name today —
        // future work threads the actual model into the id. For now
        // this test pins the current behaviour so a refactor that
        // changes id() generation flags as a deliberate decision.
        let e =
            OpenAiEmbedder::new("https://api.openai.com/v1", "test-key").with_model("text-embedding-3-large");
        assert_eq!(e.id(), "openai:text-embedding-3-small");
    }
}
