//! Phase 4.4 steering hook — alpha schedule, gating strategies, and the
//! `SteeringHook` that implements `LayerHook`.
//!
//! **Tier 1 fix #5 — entropy gating placement**: `EntropyGated` is enforced by
//! the *outer caller* (the trader's constrained-generation loop), not inside
//! `apply()`. Inside `apply()`, `EntropyGated` is treated identically to
//! `Always` for the residual transform; the gate decision is fed in by the
//! caller via `HookContext::last_logits`. This keeps the hook stateless with
//! respect to the action-token decision and lets the outer loop compute entropy
//! at the logits level (immediately after the `"action":` choice token) rather
//! than inspecting the hidden state inside the hook.
//!
//! **Tier 2 fix #7 — backtest gate logging**: `SteeringHook::drain_log()`
//! returns and clears a log of `(LayerIndex, token_index, magnitude)` tuples
//! recorded during `apply()`. Backtest mode reads this log; it does NOT
//! re-run the forward pass with a dampened vector.

use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

use candle_core::Tensor;
use xianvec_core::{LayerIndex, VectorRef};
use xianvec_inference::hooks::{HookContext, LayerHook};
use xianvec_inference::substrate::VectorBundle;

// ---------------------------------------------------------------------------
// AlphaSchedule
// ---------------------------------------------------------------------------

/// How steering magnitude evolves over the token sequence.
#[derive(Debug, Clone)]
pub enum AlphaSchedule {
    /// Fixed magnitude for every token.
    Constant(f32),
    /// Cosine oscillation: `amplitude * cos(2π * token_index / period_tokens)`.
    Cosine {
        amplitude: f32,
        period_tokens: u32,
    },
}

impl AlphaSchedule {
    /// Magnitude at `token_index` (0-based completion token index).
    pub fn current(&self, token_index: u32) -> f32 {
        match self {
            AlphaSchedule::Constant(a) => *a,
            AlphaSchedule::Cosine {
                amplitude,
                period_tokens,
            } => {
                let phase =
                    2.0 * PI * token_index as f32 / (*period_tokens).max(1) as f32;
                amplitude * phase.cos()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GatingStrategy
// ---------------------------------------------------------------------------

/// When to apply the steering vector at a given token position.
#[derive(Debug, Clone)]
pub enum GatingStrategy {
    /// Apply unconditionally.
    Always,
    /// Apply when the entropy of `last_logits` over `token_set` exceeds
    /// `threshold`. Enforcement is by the *outer caller* (Tier 1 fix #5);
    /// inside `apply()` this is treated as `Always`.
    EntropyGated {
        /// Token IDs of the set over which entropy is measured.
        token_set: Vec<u32>,
        threshold: f32,
    },
    /// Apply when the cosine similarity of the residual stream to `condition`
    /// exceeds `threshold`.
    CastGated {
        condition: VectorRef,
        threshold: f32,
    },
}

impl GatingStrategy {
    /// Evaluate the gate. Returns a scalar in `[0.0, 1.0]`.
    ///
    /// - `Always` → 1.0
    /// - `EntropyGated` → 1.0 (caller enforces; see module doc)
    /// - `CastGated` → cosine(residual, condition) ≥ threshold ? 1.0 : 0.0
    pub fn evaluate(
        &self,
        residual: &Tensor,
        condition_bundle: Option<&Tensor>,
    ) -> candle_core::Result<f32> {
        match self {
            GatingStrategy::Always => Ok(1.0),
            GatingStrategy::EntropyGated { .. } => {
                // Enforced by outer caller; act as Always here.
                Ok(1.0)
            }
            GatingStrategy::CastGated { threshold, .. } => {
                if let Some(cond) = condition_bundle {
                    let cos = cosine_similarity(residual, cond)?;
                    Ok(if cos >= *threshold { 1.0 } else { 0.0 })
                } else {
                    Ok(0.0)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SteeringEntry
// ---------------------------------------------------------------------------

/// One vector-plus-schedule-plus-gate triple applied at a specific layer.
pub struct SteeringEntry {
    pub bundle: Arc<VectorBundle>,
    pub alpha: AlphaSchedule,
    pub gate: GatingStrategy,
    pub layer: LayerIndex,
    /// Optional condition tensor for `CastGated` (pre-loaded onto device).
    pub condition_tensor: Option<Arc<Tensor>>,
}

// ---------------------------------------------------------------------------
// GateLog
// ---------------------------------------------------------------------------

/// Per-token gate magnitude log. Populated by `SteeringHook::apply()`.
/// Read via `SteeringHook::drain_log()`.
#[derive(Debug, Default, Clone)]
pub struct GateLog {
    /// `(layer, token_index, effective_magnitude)` triples in call order.
    pub magnitudes: Vec<(LayerIndex, u32, f32)>,
}

// ---------------------------------------------------------------------------
// SteeringHook
// ---------------------------------------------------------------------------

/// Implements `LayerHook` by applying one or more steering vectors at their
/// designated layers, gated by the configured `GatingStrategy`.
pub struct SteeringHook {
    pub vectors: Vec<SteeringEntry>,
    log: Mutex<GateLog>,
}

impl SteeringHook {
    pub fn new(vectors: Vec<SteeringEntry>) -> Self {
        Self {
            vectors,
            log: Mutex::new(GateLog::default()),
        }
    }

    /// Drain and return the accumulated gate log, resetting it to empty.
    pub fn drain_log(&self) -> GateLog {
        let mut guard = self.log.lock().expect("gate log lock poisoned");
        std::mem::take(&mut *guard)
    }
}

impl LayerHook for SteeringHook {
    fn apply(
        &self,
        layer_idx: usize,
        residual: &Tensor,
        ctx: &HookContext,
    ) -> candle_core::Result<Tensor> {
        let mut output = residual.clone();

        for entry in &self.vectors {
            if entry.layer.0 as usize != layer_idx {
                continue;
            }

            let g = entry
                .gate
                .evaluate(&output, entry.condition_tensor.as_deref())?;

            if g == 0.0 {
                continue;
            }

            let alpha = entry.alpha.current(ctx.token_index);
            let scale = g * alpha;

            if scale.abs() < f32::EPSILON {
                continue;
            }

            // output = output + scale * vec
            let scaled = entry.bundle.tensor.affine(scale as f64, 0.0)?;
            output = output.broadcast_add(&scaled)?;

            // Log the gate magnitude.
            let mut log_guard = self.log.lock().expect("gate log lock poisoned");
            log_guard
                .magnitudes
                .push((entry.layer, ctx.token_index, scale));
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn cosine_similarity(a: &Tensor, b: &Tensor) -> candle_core::Result<f32> {
    let a_flat = a.flatten_all()?;
    let b_flat = b.flatten_all()?;
    let dot = a_flat.mul(&b_flat)?.sum_all()?.to_scalar::<f32>()?;
    let norm_a = a_flat.sqr()?.sum_all()?.sqrt()?.to_scalar::<f32>()?;
    let norm_b = b_flat.sqr()?.sum_all()?.sqrt()?.to_scalar::<f32>()?;
    let denom = norm_a * norm_b;
    if denom < 1e-9 {
        Ok(0.0)
    } else {
        Ok(dot / denom)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{DType, Device};
    use xianvec_core::{LayerIndex, Manifest};
    use xianvec_inference::substrate::load_vector;

    fn make_bundle(dim: usize, device: &Device) -> Arc<VectorBundle> {
        use chrono::TimeZone;
        let data: Vec<f32> = (0..dim).map(|i| i as f32 / dim as f32).collect();
        let tensor = Tensor::from_vec(data, (dim,), device).unwrap();
        Arc::new(VectorBundle {
            manifest: Manifest {
                model_id: "test".into(),
                model_quant: "q4".into(),
                layer: LayerIndex(0),
                contrast_pair_set_hash: "hash".into(),
                alpha_curve_hash: "unspecified".into(),
                embedder_version: "v1".into(),
                derived_at: chrono::Utc
                    .timestamp_opt(0, 0)
                    .single()
                    .unwrap(),
            },
            tensor,
        })
    }

    #[test]
    fn alpha_schedule_constant() {
        let a = AlphaSchedule::Constant(1.5);
        assert_eq!(a.current(0), 1.5);
        assert_eq!(a.current(100), 1.5);
    }

    #[test]
    fn alpha_schedule_cosine_period() {
        // At token 0, cosine is 1 → amplitude.
        let a = AlphaSchedule::Cosine {
            amplitude: 2.0,
            period_tokens: 10,
        };
        let v0 = a.current(0);
        assert!((v0 - 2.0).abs() < 1e-5, "cos(0) should be 2.0, got {v0}");
        // At half-period, cosine is -1 → -amplitude.
        let v5 = a.current(5);
        assert!((v5 + 2.0).abs() < 1e-4, "cos(π) should be -2.0, got {v5}");
    }

    #[test]
    fn steering_hook_applies_at_correct_layer() {
        let device = Device::Cpu;
        let dim = 16usize;
        let bundle = make_bundle(dim, &device);

        let entry = SteeringEntry {
            bundle: bundle.clone(),
            alpha: AlphaSchedule::Constant(1.0),
            gate: GatingStrategy::Always,
            layer: LayerIndex(3),
            condition_tensor: None,
        };

        let hook = SteeringHook::new(vec![entry]);
        let residual = Tensor::zeros((dim,), DType::F32, &device).unwrap();
        let ctx = HookContext::new(0);

        // Hook at layer 3 — should modify.
        let out_3 = hook.apply(3, &residual, &ctx).unwrap();
        let out_3_vec: Vec<f32> = out_3.to_vec1().unwrap();
        let bundle_vec: Vec<f32> = bundle.tensor.to_vec1().unwrap();
        assert_eq!(out_3_vec, bundle_vec, "layer 3 should be steered");

        // Hook at layer 5 — should be identity.
        let out_5 = hook.apply(5, &residual, &ctx).unwrap();
        let out_5_vec: Vec<f32> = out_5.to_vec1().unwrap();
        assert_eq!(
            out_5_vec,
            vec![0.0f32; dim],
            "layer 5 should be unchanged"
        );
    }

    #[test]
    fn steering_hook_gate_log_populated() {
        let device = Device::Cpu;
        let dim = 16usize;
        let bundle = make_bundle(dim, &device);

        let entry = SteeringEntry {
            bundle,
            alpha: AlphaSchedule::Constant(1.0),
            gate: GatingStrategy::Always,
            layer: LayerIndex(3),
            condition_tensor: None,
        };

        let hook = SteeringHook::new(vec![entry]);
        let residual = Tensor::zeros((dim,), DType::F32, &device).unwrap();

        for i in 0..5u32 {
            let ctx = HookContext::new(i);
            hook.apply(3, &residual, &ctx).unwrap();
        }

        let log = hook.drain_log();
        assert_eq!(log.magnitudes.len(), 5, "should have 5 log entries");
        assert!(
            log.magnitudes.iter().all(|(_, _, m)| *m == 1.0),
            "all magnitudes should be 1.0"
        );
        // Log should be drained.
        assert!(hook.drain_log().magnitudes.is_empty());
    }

    #[test]
    fn load_spike_fixture_and_steer() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("data/vectors/spike_decisive_v1.npz");

        if !path.exists() {
            eprintln!("SKIP: spike fixture not found");
            return;
        }

        use chrono::TimeZone;
        let expected = Manifest {
            model_id: "Qwen/Qwen3-32B".into(),
            model_quant: "unspecified".into(),
            layer: LayerIndex(20),
            contrast_pair_set_hash: "6e91738f726ff205".into(),
            alpha_curve_hash: "unspecified".into(),
            embedder_version: "mlx-lm".into(),
            derived_at: chrono::Utc
                .timestamp_opt(0, 0)
                .single()
                .unwrap(),
        };

        let device = Device::Cpu;
        let bundle = load_vector(&path, &expected, &device)
            .expect("should load spike fixture");
        let bundle = Arc::new(bundle);

        let entry = SteeringEntry {
            bundle: bundle.clone(),
            alpha: AlphaSchedule::Constant(1.0),
            gate: GatingStrategy::Always,
            layer: LayerIndex(20),
            condition_tensor: None,
        };

        let hook = SteeringHook::new(vec![entry]);

        // Simulate 32 tokens of decode at layer 20 with a 5120-dim residual.
        let residual = Tensor::zeros((5120,), DType::F32, &device).unwrap();
        for i in 0..32u32 {
            let ctx = HookContext::new(i);
            hook.apply(20, &residual, &ctx).unwrap();
        }

        let log = hook.drain_log();
        assert!(
            !log.magnitudes.is_empty(),
            "gate log should have entries after 32 tokens"
        );
        assert_eq!(log.magnitudes.len(), 32);
    }
}
