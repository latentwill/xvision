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
    /// Model-aware embedder id (`openaicompat:<model>`), recomputed whenever
    /// the model changes. The memory store keys observations by this id so
    /// embeddings from different models (nomic vs qwen vs openai) never get
    /// compared in the same vector space.
    id: String,
}

fn embedder_id_for(model: &str) -> String {
    format!("openaicompat:{model}")
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
            id: embedder_id_for(DEFAULT_MODEL),
        }
    }

    /// Override the embedding model. The reported `id()` becomes
    /// `openaicompat:<model>` so the store distinguishes embedders; the
    /// reported `dim()` stays `DEFAULT_DIM` (metadata only — the store
    /// uses the real returned vector length, so a "wrong" dim is cosmetic).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self.id = embedder_id_for(&self.model);
        self
    }
}

impl OpenAiEmbedder {
    /// Build the `POST {base_url}/embeddings` request for `text`. The
    /// `Authorization: Bearer` header is attached ONLY when `api_key` is
    /// non-empty — a no-auth local server (Ollama, llama.cpp) must not
    /// receive a bogus empty bearer. Pulled out of `embed` so the header
    /// policy is unit-testable without real HTTP.
    fn build_embeddings_request(&self, text: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": &self.model,
            "input": text,
        });
        let mut req = self.client.post(url).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        req
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
        let resp: EmbeddingsResponse = self
            .build_embeddings_request(text)
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
        &self.id
    }

    /// `DEFAULT_DIM` (1536) is metadata only — used solely in tests. The
    /// memory store records the real length of the vector returned by
    /// `embed`, so a model whose true dim differs (e.g. nomic = 768) still
    /// round-trips correctly; this value is not authoritative.
    fn dim(&self) -> usize {
        DEFAULT_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedder_reports_model_aware_default_id_and_dim() {
        let e = OpenAiEmbedder::new("https://api.openai.com/v1", "test-key");
        assert_eq!(e.id(), "openaicompat:text-embedding-3-small");
        assert_eq!(e.dim(), 1536);
    }

    #[test]
    fn with_model_threads_model_into_id() {
        // id() now reflects the actual model so the store keeps embeddings
        // from different models in separate vector spaces.
        let e = OpenAiEmbedder::new("https://api.openai.com/v1", "test-key")
            .with_model("nomic-embed-text");
        assert_eq!(e.id(), "openaicompat:nomic-embed-text");

        let e2 = OpenAiEmbedder::new("http://localhost:11434/v1", "")
            .with_model("qwen3-embedding");
        assert_eq!(e2.id(), "openaicompat:qwen3-embedding");
    }

    #[test]
    fn omits_auth_header_when_key_empty() {
        // A no-auth local server (Ollama) must NOT receive an Authorization
        // header — an empty bearer can confuse some servers. Construct the
        // request and assert the header is absent.
        let e = OpenAiEmbedder::new("http://localhost:11434/v1", "")
            .with_model("nomic-embed-text");
        let req = e.build_embeddings_request("hello").build().unwrap();
        assert!(
            req.headers().get(reqwest::header::AUTHORIZATION).is_none(),
            "no-auth (empty key) endpoint must not send an Authorization header"
        );
    }

    #[test]
    fn attaches_auth_header_when_key_present() {
        let e = OpenAiEmbedder::new("https://api.openai.com/v1", "sk-live")
            .with_model("text-embedding-3-small");
        let req = e.build_embeddings_request("hello").build().unwrap();
        let auth = req
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("authenticated endpoint must send an Authorization header");
        assert_eq!(auth, "Bearer sk-live");
    }
}
