# ADR 0002 — Vector validation spike outcome

**Date:** 2026-05-03
**Status:** In progress (validation running)
**Phase:** 0.3 (CRITICAL GATE)

## Question

Do disposition-style steering vectors actually steer Qwen3-32B at 4-bit
quantization in a way that meets the 8 pass criteria from
implementation-plan.md §0.3?

## Setup

- **Model:** `mlx-community/Qwen3-32B-4bit` (37 GB on disk; 64 transformer
  layers, hidden_size 5120). Same family as the production runtime model;
  ADR 0001 documents why this and why MLX/Python for Phase 0 + candle for
  Phase 4+.
- **Toy axis:** decisive vs hedging (commits to a call vs adds qualifiers).
  Plan §0.3 calls for a toy axis to validate the methodology before
  investing in the trading-domain Conviction axis.
- **Contrast pairs:** 30 paired prompts in
  `tools/extract_vectors/spike/contrast_pairs.json`. Templated after Mitra
  §7.5 / plan §4.1.
- **Extraction:** `tools/extract_vectors/spike/extract.py` runs each pair
  through MLX with a class-swap hook that captures the residual stream at
  the *last token position* of the named layers (20, 32, 42, 50 — early /
  middle / Mitra-sweet-spot / late). Per-layer `vector = mean(positives) -
  mean(negatives)`. Output: `data/vectors/spike_decisive_v1.npz`.
- **Validation:** `tools/extract_vectors/spike/validate.py` runs 20 holdout
  prompts × magnitudes [-2, -1, 0, +1, +2] = 100 forward passes per layer.
  For each generation: hedge/decisive lexical score, residual norms,
  vector–residual cosine.

## Extraction diagnostics (2026-05-03 16:xx UTC)

```
L20: diff_norm=67.7   pos_norm=126.1   neg_norm=127.0
L32: diff_norm=84.4   pos_norm=157.5   neg_norm=181.6
L42: diff_norm=87.3   pos_norm=183.0   neg_norm=187.3
L50: diff_norm=142.2  pos_norm=399.9   neg_norm=409.1
```

All four layers show a meaningful diff norm relative to the residual norm
(≥40% of mean residual norm). L50 shows a stronger residual norm (typical of
late-layer residual stream growth) but still has the largest diff norm in
absolute terms.

## Pass criteria results (TBD — fills in when validate.py finishes)

| # | Criterion | Threshold | L42 result | Pass? |
|---|---|---|---|---|
| 1 | Directional match rate | ≥ 0.80 | TBD | TBD |
| 2 | Coherence violation rate | < 0.10 | TBD | TBD |
| 3 | Q4 persistence | inherits from (1)+(2) | TBD | TBD |
| 4 | Capability floor (MMLU) | ≤ 2pt drop | deferred to follow-up | SKIP |
| 5 | Logit lens shift | clear shift | indirect via (1) | TBD |
| 6 | Non-monotonic past threshold | score(α=2) − score(α=1) < 0.15 | TBD | TBD |
| 7 | Residual norm shift | > 1e-3 | TBD | TBD |
| 8 | Vector–residual cosine bounded | \|cos\| < 0.95 | TBD | TBD |

## Decision

TBD — see `data/probes/spike/summary.json` for the machine-readable verdict.

### If PASS

Proceed to Phase 4 with the Conviction axis on Qwen3-32B at 4-bit. Phase 4.3
hard gate becomes a regression test against the candle runtime.

### If FAIL

Options ordered by cost:
1. Try a different layer (L32 or L20 — earlier-layer steering may be cleaner
   for some axes).
2. Increase contrast pair count to 100 (Mitra: pairs scale matters at large N).
3. Switch from mean-difference to first-PC extraction (repeng's default).
4. Re-run extraction at Q8 or bf16 on rented GPU and re-check Q4 persistence
   under (3).
5. Pivot to a non-vector approach (prompt prefix tuning, fine-tuning).

## Artifacts

- `tools/extract_vectors/spike/contrast_pairs.json`
- `tools/extract_vectors/spike/extract.py`
- `tools/extract_vectors/spike/validate.py`
- `data/vectors/spike_decisive_v1.npz` + `.manifest.json`
- `data/probes/spike/results.json` (per-prompt × magnitude grid)
- `data/probes/spike/summary.json` (8-criterion verdict)
