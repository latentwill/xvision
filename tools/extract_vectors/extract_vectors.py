"""Phase 4.2 — Production steering-vector extraction utility.

Loads a transformers model (default: Qwen/Qwen3-32B, fp16), runs each
contrastive pair through it with per-layer residual-capture hooks, computes
per-layer steering vectors, and writes:

    <out>.npz                — steering vectors, {f"L{n}": fp32 array}
    <out>.manifest.json      — contract manifest (Rust substrate.rs compatible)
    <out>_random.npz         — Gaussian control, Frobenius-norm-matched
    <out>_random.manifest.json
    <out>_orth.npz           — orthogonal control (subtracted real projection)
    <out>_orth.manifest.json

Manifest contract (must match crates/xianvec-core/src/substrate.rs):
    model_id              string
    model_quant           string  ("fp16" | "bf16" | ...)
    layer                 number  (LayerIndex newtype serializes as u16)
    contrast_pair_set_hash  string (64-char sha256)
    alpha_curve_hash        string (sha256 of alpha schedule JSON)
    embedder_version        string ("transformers-X.Y.Z+repeng-A.B.C")
    derived_at              string (RFC3339 UTC)

NOTE: One manifest is written *per layer*.  The .npz bundles all layers for
convenience; manifests are per-layer sidecars because the Rust contract
contract binds to a single LayerIndex.  A combined manifest listing all layers
is also written as <out>.manifest.json for human review, but the per-layer
sidecars are the load-contract artifacts.

Usage:
    python extract_vectors.py \\
        --model Qwen/Qwen3-32B \\
        --spec specs/conviction.yaml \\
        --layers 20,22,24 \\
        --out data/vectors/conviction_v1 \\
        --device cuda \\
        --dtype fp16
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import numpy as np

# ---------------------------------------------------------------------------
# Imports guarded so --help works without torch installed
# ---------------------------------------------------------------------------
try:
    import torch
    import transformers
    _TORCH_AVAILABLE = True
except ImportError:
    _TORCH_AVAILABLE = False

try:
    import repeng
    _REPENG_VERSION = getattr(repeng, "__version__", "unknown")
except ImportError:
    repeng = None  # type: ignore[assignment]
    _REPENG_VERSION = "not-installed"

from datasets import generate_pairs, load_spec, pair_set_hash  # noqa: E402


# ---------------------------------------------------------------------------
# Manifest helpers
# ---------------------------------------------------------------------------

_ALPHA_CONSTANT_V1 = {"type": "constant", "alpha": 1.0}


def _alpha_curve_hash(schedule: dict) -> str:
    serialized = json.dumps(schedule, sort_keys=True)
    return hashlib.sha256(serialized.encode()).hexdigest()


def _embedder_version() -> str:
    tv = transformers.__version__ if _TORCH_AVAILABLE else "unknown"
    return f"transformers-{tv}+repeng-{_REPENG_VERSION}"


def _make_manifest(
    *,
    model_id: str,
    model_quant: str,
    layer: int,
    contrast_hash: str,
    derived_at: str,
    variant: str = "",  # "", "_random", "_orth"
) -> dict[str, Any]:
    base: dict[str, Any] = {
        "model_id": model_id,
        "model_quant": model_quant,
        "layer": layer,
        "contrast_pair_set_hash": contrast_hash,
        "alpha_curve_hash": _alpha_curve_hash(_ALPHA_CONSTANT_V1),
        "embedder_version": _embedder_version(),
        "derived_at": derived_at,
    }
    if variant:
        base["control_variant"] = variant
    return base


# ---------------------------------------------------------------------------
# Extraction
# ---------------------------------------------------------------------------

def _extract_residuals(
    model: "torch.nn.Module",
    tokenizer: Any,
    pairs: list[dict[str, str]],
    layer_indices: list[int],
    device: str,
) -> tuple[dict[int, list[np.ndarray]], dict[int, list[np.ndarray]]]:
    """Run forward passes and capture last-token residuals per layer.

    Returns (pos_residuals, neg_residuals) where each is a dict mapping
    layer_index → list of (hidden_dim,) float32 arrays.
    """
    import torch  # local to keep --help working

    # Identify transformer blocks.  HF Qwen3 exposes model.model.layers
    base = getattr(model, "model", model)
    blocks = list(base.layers)

    n_blocks = len(blocks)
    for li in layer_indices:
        if li >= n_blocks:
            raise ValueError(f"Layer {li} out of range (model has {n_blocks} blocks)")

    pos_caps: dict[int, list[np.ndarray]] = {li: [] for li in layer_indices}
    neg_caps: dict[int, list[np.ndarray]] = {li: [] for li in layer_indices}

    def _make_hook(li: int, target: dict[int, list[np.ndarray]]):
        def hook(module, input, output):
            # output may be a tuple; first element is the hidden state tensor
            hs = output[0] if isinstance(output, tuple) else output
            # shape: (batch, seq_len, hidden_dim); take last token
            last = hs[0, -1, :].detach().float().cpu().numpy()
            target[li].append(last)
        return hook

    handles = []
    for li in layer_indices:
        h_pos = blocks[li].register_forward_hook(_make_hook(li, pos_caps))
        handles.append(h_pos)

    # We run the same hook twice (pos then neg); we distinguish by run order
    # so we only register once and reuse the same capture lists.
    # Strategy: temporarily swap target dict on each run.
    for handle in handles:
        handle.remove()
    handles.clear()

    # Register once per layer into a "current_target" indirection
    current_targets: dict[int, dict] = {li: pos_caps for li in layer_indices}

    def _make_indirect_hook(li: int):
        def hook(module, input, output):
            hs = output[0] if isinstance(output, tuple) else output
            last = hs[0, -1, :].detach().float().cpu().numpy()
            current_targets[li][li].append(last)
        return hook

    for li in layer_indices:
        h = blocks[li].register_forward_hook(_make_indirect_hook(li))
        handles.append(h)

    try:
        for i, pair in enumerate(pairs):
            for label, target_dict in (("positive", pos_caps), ("negative", neg_caps)):
                for li in layer_indices:
                    current_targets[li] = target_dict

                prompt = pair[label]
                inputs = tokenizer(prompt, return_tensors="pt").to(device)
                with torch.no_grad():
                    model(**inputs)

            if (i + 1) % 10 == 0 or i == len(pairs) - 1:
                print(f"  pair {i + 1}/{len(pairs)}", flush=True)
    finally:
        for h in handles:
            h.remove()

    return pos_caps, neg_caps


def _compute_vectors(
    pos_caps: dict[int, list[np.ndarray]],
    neg_caps: dict[int, list[np.ndarray]],
    layer_indices: list[int],
    method: str,
) -> dict[int, np.ndarray]:
    """Compute per-layer steering vectors from captured residuals."""
    vectors: dict[int, np.ndarray] = {}
    for li in layer_indices:
        pos = np.stack(pos_caps[li])  # (n_pairs, hidden)
        neg = np.stack(neg_caps[li])

        if method == "meandiff":
            vec = pos.mean(axis=0) - neg.mean(axis=0)
        elif method == "first-pc":
            diffs = pos - neg  # (n_pairs, hidden)
            # first PC via SVD
            _, _, Vt = np.linalg.svd(diffs, full_matrices=False)
            vec = Vt[0]  # (hidden,)
            # orient: if dot with mean-diff is negative, flip sign
            mean_diff = pos.mean(axis=0) - neg.mean(axis=0)
            if np.dot(vec, mean_diff) < 0:
                vec = -vec
        else:
            raise ValueError(f"Unknown method: {method}")

        vectors[li] = vec.astype(np.float32)
    return vectors


def _build_random_control(
    real_vectors: dict[int, np.ndarray],
    rng: np.random.Generator,
) -> dict[int, np.ndarray]:
    """Gaussian noise scaled to match Frobenius norm of each real vector."""
    controls: dict[int, np.ndarray] = {}
    for li, vec in real_vectors.items():
        target_norm = float(np.linalg.norm(vec))
        noise = rng.standard_normal(vec.shape).astype(np.float32)
        noise_norm = float(np.linalg.norm(noise))
        if noise_norm > 0:
            noise = noise * (target_norm / noise_norm)
        controls[li] = noise
    return controls


def _build_orth_control(
    real_vectors: dict[int, np.ndarray],
    rng: np.random.Generator,
) -> dict[int, np.ndarray]:
    """Random vector orthogonalized against real, renormalized to same norm.

    Verified: |cos(real, orth)| <= 0.05 (asserted).
    """
    controls: dict[int, np.ndarray] = {}
    for li, vec in real_vectors.items():
        target_norm = float(np.linalg.norm(vec))
        unit_real = vec / (target_norm + 1e-12)

        # Random direction
        candidate = rng.standard_normal(vec.shape).astype(np.float32)
        # Subtract projection onto real
        candidate = candidate - np.dot(candidate, unit_real) * unit_real
        cand_norm = float(np.linalg.norm(candidate))
        if cand_norm > 0:
            candidate = candidate * (target_norm / cand_norm)

        # Verify orthogonality
        cos_sim = float(
            np.dot(candidate, vec) / (np.linalg.norm(candidate) * np.linalg.norm(vec) + 1e-12)
        )
        assert abs(cos_sim) <= 0.05, (
            f"Layer {li}: orthogonal control has |cos| = {abs(cos_sim):.4f} > 0.05"
        )
        controls[li] = candidate
    return controls


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Extract steering vectors from a contrast spec using transformers + repeng."
    )
    p.add_argument("--model", default="Qwen/Qwen3-32B",
                   help="HuggingFace model id or local path.")
    p.add_argument("--spec", required=True,
                   help="Path to the axis YAML spec (or materialized pairs JSON).")
    p.add_argument("--layers", default="20,22,24",
                   help="Comma-separated 0-based transformer block indices.")
    p.add_argument("--out", required=True,
                   help="Output prefix (no extension). Writes <prefix>.npz + .manifest.json.")
    p.add_argument("--device", choices=["cpu", "cuda", "mps"], default="cpu")
    p.add_argument("--method", choices=["meandiff", "first-pc"], default="meandiff",
                   help="Vector computation method (default: meandiff).")
    p.add_argument("--dtype", choices=["fp16", "bf16"], default="fp16",
                   help="Model weight dtype (default: fp16).")
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    if not _TORCH_AVAILABLE:
        print("ERROR: torch and transformers are required. Install via requirements.txt.",
              file=sys.stderr)
        return 1

    import torch  # local import

    layer_indices = sorted(int(x) for x in args.layers.split(","))
    out_prefix = Path(args.out)
    out_prefix.parent.mkdir(parents=True, exist_ok=True)

    # Load spec / pairs
    spec_path = Path(args.spec)
    if spec_path.suffix in {".yaml", ".yml"}:
        spec = load_spec(spec_path)
        pairs = generate_pairs(spec)
    elif spec_path.suffix == ".json":
        payload = json.loads(spec_path.read_text())
        pairs = payload["pairs"]
    else:
        print(f"ERROR: --spec must be .yaml or .json, got {spec_path.suffix}", file=sys.stderr)
        return 1

    contrast_hash = pair_set_hash(pairs)
    print(f"Loaded {len(pairs)} pairs.  Hash: {contrast_hash}", flush=True)

    # Resolve torch dtype
    torch_dtype = torch.float16 if args.dtype == "fp16" else torch.bfloat16

    print(f"Loading {args.model} ({args.dtype}) ...", flush=True)
    tokenizer = transformers.AutoTokenizer.from_pretrained(
        args.model, trust_remote_code=True
    )
    model = transformers.AutoModelForCausalLM.from_pretrained(
        args.model,
        torch_dtype=torch_dtype,
        device_map=args.device,
        trust_remote_code=True,
    )
    model.eval()

    print(f"Extracting residuals at layers {layer_indices} ...", flush=True)
    pos_caps, neg_caps = _extract_residuals(
        model, tokenizer, pairs, layer_indices, args.device
    )

    vectors = _compute_vectors(pos_caps, neg_caps, layer_indices, args.method)

    derived_at = datetime.now(timezone.utc).isoformat()
    emb_ver = _embedder_version()

    # --- Real vectors ---
    npz_path = out_prefix.with_suffix(".npz")
    np.savez(npz_path, **{f"L{li}": v for li, v in vectors.items()})
    print(f"Wrote {npz_path}", flush=True)

    # Per-layer sidecars (one per layer — each is a valid Rust manifest)
    for li, vec in vectors.items():
        manifest = _make_manifest(
            model_id=args.model,
            model_quant=args.dtype,
            layer=li,
            contrast_hash=contrast_hash,
            derived_at=derived_at,
        )
        sidecar = out_prefix.parent / f"{out_prefix.stem}_L{li}.manifest.json"
        sidecar.write_text(json.dumps(manifest, indent=2))
        print(f"Wrote {sidecar}", flush=True)

    # Combined manifest (human review; not a Rust load target)
    combined = {
        "model_id": args.model,
        "model_quant": args.dtype,
        "layers": layer_indices,
        "n_pairs": len(pairs),
        "contrast_pair_set_hash": contrast_hash,
        "alpha_curve_hash": _alpha_curve_hash(_ALPHA_CONSTANT_V1),
        "embedder_version": emb_ver,
        "derived_at": derived_at,
        "method": args.method,
    }
    combined_path = out_prefix.with_suffix(".manifest.json")
    combined_path.write_text(json.dumps(combined, indent=2))
    print(f"Wrote {combined_path}", flush=True)

    # --- Random control ---
    rng = np.random.default_rng(seed=42)
    rand_vectors = _build_random_control(vectors, rng)
    rand_path = Path(str(out_prefix) + "_random.npz")
    np.savez(rand_path, **{f"L{li}": v for li, v in rand_vectors.items()})
    print(f"Wrote {rand_path}", flush=True)

    for li in layer_indices:
        manifest = _make_manifest(
            model_id=args.model,
            model_quant=args.dtype,
            layer=li,
            contrast_hash=contrast_hash,
            derived_at=derived_at,
            variant="random",
        )
        sidecar = out_prefix.parent / f"{out_prefix.stem}_random_L{li}.manifest.json"
        sidecar.write_text(json.dumps(manifest, indent=2))

    rand_combined_path = Path(str(out_prefix) + "_random.manifest.json")
    rand_combined = {**combined, "control_variant": "random", "layers": layer_indices}
    rand_combined_path.write_text(json.dumps(rand_combined, indent=2))
    print(f"Wrote {rand_combined_path}", flush=True)

    # --- Orthogonal control ---
    rng2 = np.random.default_rng(seed=43)
    orth_vectors = _build_orth_control(vectors, rng2)
    orth_path = Path(str(out_prefix) + "_orth.npz")
    np.savez(orth_path, **{f"L{li}": v for li, v in orth_vectors.items()})
    print(f"Wrote {orth_path}", flush=True)

    for li in layer_indices:
        manifest = _make_manifest(
            model_id=args.model,
            model_quant=args.dtype,
            layer=li,
            contrast_hash=contrast_hash,
            derived_at=derived_at,
            variant="orthogonal",
        )
        sidecar = out_prefix.parent / f"{out_prefix.stem}_orth_L{li}.manifest.json"
        sidecar.write_text(json.dumps(manifest, indent=2))

    orth_combined_path = Path(str(out_prefix) + "_orth.manifest.json")
    orth_combined = {**combined, "control_variant": "orthogonal", "layers": layer_indices}
    orth_combined_path.write_text(json.dumps(orth_combined, indent=2))
    print(f"Wrote {orth_combined_path}", flush=True)

    print("\nDone.", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
