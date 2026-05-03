//! Qwen3 Q4 GGUF inference engine. Owns the candle model weights, tokenizer,
//! and sampling loop.
//!
//! ## Hook injection — RESOLVED (ADR 0007, Option A)
//!
//! `quantized_qwen3::ModelWeights::forward` iterates a private `Vec<LayerWeights>`.
//! We work around this by mirroring the upstream struct layout in
//! `crate::vendor_qwen3::ModelWeightsMirror` and transmuting a `&mut ModelWeights`
//! reference to drive the layer loop ourselves. The transmute is guarded by a
//! `size_of` + `align_of` runtime assertion; if upstream changes its layout the
//! engine panics with a diagnostic message rather than silently miscomputing.
//!
//! The generation loop now calls `vendor_qwen3::forward_with_hooks(...)` which
//! fires `hook.apply(layer_idx, &residual, ctx)` after every transformer block.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_qwen3::ModelWeights as Qwen3Model;
use thiserror::Error;
use tokenizers::Tokenizer;
use tracing::info;

use crate::hooks::{HookContext, IdentityHook, LayerHook};
use crate::vendor_qwen3;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("candle: {0}")]
    Candle(#[from] candle_core::Error),
    #[error("tokenizer: {0}")]
    Tokenizer(String),
    #[error("model load failed: {0}")]
    Load(String),
}

pub struct Qwen3Engine {
    model: Qwen3Model,
    tokenizer: Tokenizer,
    device: Device,
    eos_token: u32,
    /// Per-layer hooks. v1 maps layer index → hook. The hook is called after
    /// each block with the residual tensor (see HOOK_STATUS above for the
    /// current limitation).
    hooks: BTreeMap<u16, Arc<dyn LayerHook>>,
    /// Single-slot hook installed via `set_hook`. Takes precedence over the
    /// per-layer `hooks` map when set.
    primary_hook: Option<Box<dyn LayerHook>>,
}

#[derive(Debug, Clone)]
pub struct GenerateOpts {
    pub max_tokens: usize,
    pub temperature: f64,
    pub top_p: Option<f64>,
    pub top_k: Option<usize>,
    pub seed: u64,
    pub repeat_penalty: f32,
    pub repeat_last_n: usize,
}

impl Default for GenerateOpts {
    fn default() -> Self {
        Self {
            max_tokens: 64,
            temperature: 0.0,
            top_p: None,
            top_k: None,
            seed: 42,
            repeat_penalty: 1.1,
            repeat_last_n: 64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerateResult {
    pub text: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub prompt_dt_ms: u128,
    pub completion_dt_ms: u128,
}

impl Qwen3Engine {
    /// Auto-pick the best available device (Metal → CUDA → CPU).
    pub fn pick_device() -> Result<Device, EngineError> {
        if let Ok(d) = Device::new_metal(0) {
            info!(target: "xianvec_inference", "device: metal");
            return Ok(d);
        }
        if let Ok(d) = Device::new_cuda(0) {
            info!(target: "xianvec_inference", "device: cuda");
            return Ok(d);
        }
        info!(target: "xianvec_inference", "device: cpu");
        Ok(Device::Cpu)
    }

    /// Load a Qwen3 Q4 GGUF model from `gguf_path` with a `tokenizer.json`
    /// found at `tokenizer_path`.
    pub fn load(
        gguf_path: impl AsRef<Path>,
        tokenizer_path: impl AsRef<Path>,
        device: Device,
    ) -> Result<Self, EngineError> {
        let gguf_path: PathBuf = gguf_path.as_ref().to_path_buf();
        let mut file = std::fs::File::open(&gguf_path)?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| EngineError::Load(format!("{e:?}")))?;

        let total_bytes: usize = content
            .tensor_infos
            .values()
            .map(|t| {
                t.shape.elem_count() * t.ggml_dtype.type_size() / t.ggml_dtype.block_size()
            })
            .sum();
        info!(
            target: "xianvec_inference",
            tensors = content.tensor_infos.len(),
            size_mib = total_bytes / 1024 / 1024,
            "gguf header parsed"
        );

        let model = Qwen3Model::from_gguf(content, &mut file, &device)
            .map_err(|e| EngineError::Load(format!("{e:?}")))?;

        let tokenizer = Tokenizer::from_file(tokenizer_path.as_ref())
            .map_err(|e| EngineError::Tokenizer(format!("{e}")))?;

        let eos_token = tokenizer
            .get_vocab(true)
            .get("<|im_end|>")
            .copied()
            .ok_or_else(|| EngineError::Tokenizer("missing <|im_end|> token".into()))?;

        Ok(Self {
            model,
            tokenizer,
            device,
            eos_token,
            hooks: BTreeMap::new(),
            primary_hook: None,
        })
    }

    /// Install a hook that will be called at every layer boundary.
    ///
    /// This is the Phase 4.4 entry point. Currently the hook is stored but
    /// cannot fire until the per-block residual injection path is vendored
    /// (see HOOK_STATUS). The hook storage and call-site wiring are complete.
    pub fn set_hook(&mut self, hook: Box<dyn LayerHook>) {
        self.primary_hook = Some(hook);
    }

    /// Remove the primary hook (reverts to `IdentityHook` behaviour).
    pub fn clear_hook(&mut self) {
        self.primary_hook = None;
    }

    /// Install a per-layer hook at `layer` (legacy API; `set_hook` is preferred
    /// for Phase 4.4).
    pub fn install_hook(&mut self, layer: u16, hook: Arc<dyn LayerHook>) {
        self.hooks.insert(layer, hook);
    }

    pub fn installed_hook_layers(&self) -> Vec<u16> {
        self.hooks.keys().copied().collect()
    }

    /// Apply the active hook to `residual` at `layer_idx` with the given token
    /// context. Returns the (possibly modified) residual.
    ///
    /// Hook call order: primary hook takes precedence; falls back to per-layer
    /// hooks; falls back to identity.
    ///
    /// Used by callers that need the engine's hook dispatch outside of the main
    /// generation loop (e.g. single-shot residual inspection).
    #[allow(dead_code)]
    fn apply_hooks(
        &self,
        layer_idx: usize,
        residual: &Tensor,
        ctx: &HookContext,
    ) -> candle_core::Result<Tensor> {
        if let Some(h) = &self.primary_hook {
            return h.apply(layer_idx, residual, ctx);
        }
        if let Some(h) = self.hooks.get(&(layer_idx as u16)) {
            return h.apply(layer_idx, residual, ctx);
        }
        IdentityHook.apply(layer_idx, residual, ctx)
    }

    /// One-shot generation. Greedy when `temperature == 0.0`.
    pub fn generate(
        &mut self,
        prompt: &str,
        opts: &GenerateOpts,
    ) -> Result<GenerateResult, EngineError> {
        // Apply Qwen3 chat template, no-thinking variant.
        let formatted = format!(
            "<|im_start|>user\n{prompt}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
        let encoded = self
            .tokenizer
            .encode(formatted, true)
            .map_err(|e| EngineError::Tokenizer(e.to_string()))?;
        let prompt_tokens = encoded.get_ids().to_vec();

        let mut sampling = match opts.temperature {
            t if t <= 0.0 => Sampling::ArgMax,
            t => match (opts.top_k, opts.top_p) {
                (None, None) => Sampling::All { temperature: t },
                (Some(k), None) => Sampling::TopK { k, temperature: t },
                (None, Some(p)) => Sampling::TopP { p, temperature: t },
                (Some(k), Some(p)) => Sampling::TopKThenTopP { k, p, temperature: t },
            },
        };
        let mut logits_processor =
            LogitsProcessor::from_sampling(opts.seed, sampling.clone());

        let prompt_start = std::time::Instant::now();
        let input = Tensor::new(prompt_tokens.as_slice(), &self.device)?.unsqueeze(0)?;

        // Prefill: drive through vendor_qwen3::forward_with_hooks so the hook
        // fires once per layer even on the prompt pass (token_index = 0).
        let ctx = HookContext::new(0);
        let hook_ref: &dyn LayerHook = self
            .primary_hook
            .as_deref()
            .map(|h| h as &dyn LayerHook)
            .unwrap_or(&IdentityHook);
        let logits = vendor_qwen3::forward_with_hooks(&mut self.model, &input, 0, hook_ref, &ctx)?
            .squeeze(0)?;
        let mut next_token = logits_processor.sample(&logits)?;
        let prompt_dt_ms = prompt_start.elapsed().as_millis();

        let completion_start = std::time::Instant::now();
        let mut all_tokens = vec![next_token];

        for index in 0..opts.max_tokens.saturating_sub(1) {
            let ctx = HookContext::new(index as u32 + 1);
            let input = Tensor::new(&[next_token], &self.device)?.unsqueeze(0)?;
            // Resolve hook ref inside the loop (borrow checker: primary_hook is
            // behind &self but model is &mut self — split borrows aren't stable
            // for this pattern, so we take the hook as a raw pointer for the call).
            let hook_ref: &dyn LayerHook = self
                .primary_hook
                .as_deref()
                .map(|h| h as &dyn LayerHook)
                .unwrap_or(&IdentityHook);
            let logits = vendor_qwen3::forward_with_hooks(
                &mut self.model,
                &input,
                prompt_tokens.len() + index,
                hook_ref,
                &ctx,
            )?
            .squeeze(0)?;

            let logits = if opts.repeat_penalty == 1.0 {
                logits
            } else {
                let start_at = all_tokens.len().saturating_sub(opts.repeat_last_n);
                candle_transformers::utils::apply_repeat_penalty(
                    &logits,
                    opts.repeat_penalty,
                    &all_tokens[start_at..],
                )?
            };
            next_token = logits_processor.sample(&logits)?;
            all_tokens.push(next_token);
            if next_token == self.eos_token {
                break;
            }
            let _ = &mut sampling;
        }
        let completion_dt_ms = completion_start.elapsed().as_millis();

        let text = self
            .tokenizer
            .decode(&all_tokens, true)
            .map_err(|e| EngineError::Tokenizer(e.to_string()))?;

        Ok(GenerateResult {
            text,
            prompt_tokens: prompt_tokens.len(),
            completion_tokens: all_tokens.len(),
            prompt_dt_ms,
            completion_dt_ms,
        })
    }
}
