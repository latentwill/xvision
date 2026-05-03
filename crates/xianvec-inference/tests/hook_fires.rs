//! Integration test: verify that the hook fires on per-transformer-block
//! residuals during the candle forward pass.
//!
//! Uses a self-contained counting hook (no dependency on xianvec-introspect)
//! so this test has no circular-dependency issues.
//!
//! ## Running manually
//!
//! ```bash
//! # From the workspace root:
//! cargo test -p xianvec-inference --test hook_fires hook_fires_introspection -- --ignored --nocapture
//! ```
//!
//! The test is `#[ignore]`d by default because:
//!   1. The GGUF is ~20 GiB and not committed to the repo.
//!   2. Loading takes ~30–60 s on an M-series Mac (Metal, 4-bit dequant).
//!   3. CI does not have the model weights.
//!
//! ## Phase 4.3 hard-gate test
//!
//! `validate_directional_match_production` remains `#[ignore]`d for the same
//! reasons. To flip to non-ignored: provide a smaller fixture GGUF (e.g.
//! Qwen3-0.6B) under `models/qwen3-small-q4-gguf/` and update the path constant.
//!
//! ```bash
//! cargo test -p xianvec-inference --test hook_fires validate_directional_match_production \
//!   -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use candle_core::Tensor;
use xianvec_inference::hooks::{HookContext, LayerHook};

// ---------------------------------------------------------------------------
// CountingHook: fires on every layer, records (layer_idx, token_index) pairs.
// ---------------------------------------------------------------------------

struct CountingHook {
    calls: Arc<Mutex<Vec<(usize, u32)>>>,
}

impl CountingHook {
    fn new() -> (Self, Arc<Mutex<Vec<(usize, u32)>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (Self { calls: Arc::clone(&calls) }, calls)
    }
}

impl LayerHook for CountingHook {
    fn apply(
        &self,
        layer_idx: usize,
        residual: &Tensor,
        ctx: &HookContext,
    ) -> candle_core::Result<Tensor> {
        self.calls
            .lock()
            .expect("lock")
            .push((layer_idx, ctx.token_index));
        Ok(residual.clone())
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn gguf_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models/qwen3-32b-q4-gguf/Qwen_Qwen3-32B-Q4_K_M.gguf")
}

fn tokenizer_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models/qwen3-32b-mlx-4bit/tokenizer.json")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that the hook fires at every layer boundary for every generated token.
///
/// Assertion: total call count >= 4 * num_layers
/// (Qwen3-32B has 64 layers → expect >= 256 samples for 4 completion tokens,
/// plus num_layers more for the prefill pass.)
///
/// Marked `#[ignore]` — requires ~20 GiB GGUF + 24+ GiB RAM.
/// Run: `cargo test -p xianvec-inference --test hook_fires hook_fires_introspection -- --ignored`
#[test]
#[ignore = "requires Qwen3-32B GGUF (~20 GiB); run manually with --ignored"]
fn hook_fires_introspection() {
    use xianvec_inference::{engine::GenerateOpts, Qwen3Engine};

    let gguf = gguf_path();
    let tok = tokenizer_path();
    assert!(gguf.exists(), "GGUF not found at {}", gguf.display());
    assert!(tok.exists(), "tokenizer not found at {}", tok.display());

    let device = Qwen3Engine::pick_device().expect("device pick");
    let mut engine = Qwen3Engine::load(&gguf, &tok, device).expect("engine load");

    let (hook, calls_arc) = CountingHook::new();
    engine.set_hook(Box::new(hook));

    let opts = GenerateOpts {
        max_tokens: 4,
        temperature: 0.0,
        seed: 42,
        ..Default::default()
    };

    engine
        .generate("Say hello in one word.", &opts)
        .expect("generate");

    let calls = calls_arc.lock().expect("lock");

    assert!(
        !calls.is_empty(),
        "hook never fired — vendor_qwen3::forward_with_hooks not wired"
    );

    // Infer num_layers from the maximum layer_idx seen.
    let max_layer = calls.iter().map(|(li, _)| *li).max().unwrap_or(0);
    let num_layers = max_layer + 1;

    // We generate 4 completion tokens; plus the prefill pass fires too.
    // Conservative: require >= 4 * num_layers (ignoring prefill).
    let min_expected = 4 * num_layers;
    assert!(
        calls.len() >= min_expected,
        "expected >= {} hook calls (4 tokens × {} layers), got {}",
        min_expected,
        num_layers,
        calls.len()
    );

    // Verify layer indices cover the full range 0..num_layers-1.
    let mut seen_layers: Vec<bool> = vec![false; num_layers];
    for (li, _) in calls.iter() {
        if *li < num_layers {
            seen_layers[*li] = true;
        }
    }
    assert!(
        seen_layers.iter().all(|&v| v),
        "not all layers fired: missing indices {:?}",
        seen_layers
            .iter()
            .enumerate()
            .filter(|(_, &v)| !v)
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    );

    println!(
        "hook_fires_introspection: {} calls total, {} layers, {} tokens",
        calls.len(),
        num_layers,
        calls.iter().map(|(_, t)| t).max().unwrap_or(&0) + 1,
    );
}

/// Phase 4.3 hard-gate: steering increases decisiveness on >= 75% of holdout
/// prompts. Requires production model + spike vectors.
///
/// Run: `cargo test -p xianvec-inference --test hook_fires validate_directional_match_production -- --ignored`
#[test]
#[ignore = "requires production model + spike vectors; run manually with --ignored"]
fn validate_directional_match_production() {
    // TODO: load Qwen3Engine + spike vectors; run 5 holdout prompts with
    // steering installed; assert directional_match_rate >= 0.75.
    todo!("wire engine + production vectors");
}
