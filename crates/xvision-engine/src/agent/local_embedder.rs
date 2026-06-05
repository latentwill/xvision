//! Offline, deterministic, dependency-free embedder for the Cortex memory
//! layer — the dev/offline fallback that lets recall function without any
//! network embeddings provider.
//!
//! # Why this exists (embedder-source decision)
//!
//! Provisioning the memory embedder must NOT hard-depend on OpenAI. The
//! engine resolves an embedder source in this order (locked, Phase 0):
//!
//!   1. `XVN_MEMORY_EMBEDDER=local`           → this [`LocalEmbedder`].
//!   2. `XVN_MEMORY_EMBEDDER_PROVIDER=<name>` → that provider's
//!      OpenAI-compatible `/embeddings` endpoint (explicit operator opt-in,
//!      even when the provider is not api.openai.com).
//!   3. `OPENAI_API_KEY`                       → the historical OpenAI env
//!      path (`OPENAI_BASE_URL` overrides the host).
//!   4. Auto-detect a configured, keyed provider whose `base_url` points at
//!      the REAL api.openai.com (guaranteed to serve `/embeddings`).
//!   5. Otherwise no embedder — recall/record degrade to a no-op.
//!
//! In every case the embedding MODEL is `XVN_MEMORY_EMBEDDER_MODEL` when
//! set, otherwise `text-embedding-3-small`. See
//! [`crate::agent::embedder_choice`] for the pure resolution function.
//!
//! # What this embedder is (and is NOT)
//!
//! `LocalEmbedder` hashes the lowercased whitespace tokens of the input
//! into a fixed-dimension bag-of-tokens vector, then L2-normalizes it.
//! It is fully deterministic and offline, so two identical strings always
//! embed to the same vector and cosine similarity over these vectors is
//! stable across processes. It is, however, a LOW-QUALITY embedder — it
//! captures lexical overlap only, with no semantic understanding. It is
//! intended for development, CI, and air-gapped operation, NOT for
//! production recall quality. The engine logs a clear warning when this
//! embedder is selected.
//!
//! The dimensionality (256) is deliberately distinct from the OpenAI
//! 1536-dim model so an operator who later configures a real embedder
//! against a DB seeded under `local:hash-v1` can tell the vectors apart by
//! `embedder_id` and re-embed rather than mixing incompatible spaces.

use async_trait::async_trait;
use xvision_memory::embedder::Embedder;

/// Embedding dimensionality. Smaller than the OpenAI 1536-dim space on
/// purpose (see the module docs) — recall only needs an internally
/// consistent space, and a wide vector buys nothing for a hash embedder.
const LOCAL_DIM: usize = 256;

/// Deterministic offline embedder. See the module docs for the
/// quality/usage caveats.
pub struct LocalEmbedder {
    dim: usize,
}

impl Default for LocalEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalEmbedder {
    /// Build a `LocalEmbedder` at the default [`LOCAL_DIM`] dimensionality.
    pub fn new() -> Self {
        Self { dim: LOCAL_DIM }
    }

    /// Compute the embedding for `text`. Factored out of the trait method
    /// so the in-file tests can exercise it synchronously.
    fn embed_sync(&self, text: &str) -> Vec<f32> {
        let mut acc = vec![0.0f32; self.dim];

        // Bag-of-tokens: lowercase, split on unicode whitespace, hash each
        // token to a bucket and accumulate. A token also contributes a
        // small amount to a second bucket derived from a salted hash so two
        // tokens that collide in the primary bucket are unlikely to be
        // indistinguishable.
        for token in text.split_whitespace() {
            let lower = token.to_lowercase();
            let h = fnv1a(lower.as_bytes());
            let primary = (h as usize) % self.dim;
            acc[primary] += 1.0;

            let h2 = fnv1a_salted(lower.as_bytes(), 0x9E37_79B9);
            let secondary = (h2 as usize) % self.dim;
            acc[secondary] += 0.5;
        }

        // L2-normalize so cosine similarity is well-behaved. An all-empty
        // input (no tokens) yields the zero vector, which is fine — the
        // store treats it as "no signal".
        let norm: f32 = acc.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in acc.iter_mut() {
                *v /= norm;
            }
        }
        acc
    }
}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.embed_sync(text))
    }

    fn id(&self) -> &str {
        "local:hash-v1"
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// 64-bit FNV-1a hash. Stable across platforms and processes (no random
/// seed), which is exactly what a deterministic embedder needs.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// FNV-1a seeded with a constant salt, used to pick a second bucket per
/// token so primary-bucket collisions don't fully alias.
fn fnv1a_salted(bytes: &[u8], salt: u64) -> u64 {
    let mut hash = fnv1a(bytes) ^ salt;
    // One extra mix pass so the salt actually perturbs the distribution.
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_vector() {
        let e = LocalEmbedder::new();
        let a = e.embed_sync("buy when fear is high");
        let b = e.embed_sync("buy when fear is high");
        assert_eq!(a, b, "embedding must be deterministic for identical text");
    }

    #[test]
    fn different_text_different_vector() {
        let e = LocalEmbedder::new();
        let a = e.embed_sync("buy when fear is high");
        let b = e.embed_sync("sell into euphoric rallies");
        assert_ne!(a, b, "distinct text should produce distinct embeddings");
    }

    #[test]
    fn reports_fixed_dim() {
        let e = LocalEmbedder::new();
        assert_eq!(e.dim(), LOCAL_DIM);
        assert_eq!(e.embed_sync("anything at all").len(), LOCAL_DIM);
        // The empty string still yields a correctly-sized (zero) vector.
        assert_eq!(e.embed_sync("").len(), LOCAL_DIM);
    }

    #[test]
    fn output_is_l2_normalized_for_nonempty_input() {
        let e = LocalEmbedder::new();
        let v = e.embed_sync("alpha beta gamma alpha");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "expected unit norm, got {norm}");
    }

    #[test]
    fn case_insensitive_tokens() {
        let e = LocalEmbedder::new();
        assert_eq!(e.embed_sync("Risk OFF"), e.embed_sync("risk off"));
    }

    #[test]
    fn reports_stable_id() {
        assert_eq!(LocalEmbedder::new().id(), "local:hash-v1");
    }
}
