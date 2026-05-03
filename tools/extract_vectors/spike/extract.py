"""Phase 0.3 spike — MLX-based steering vector extraction.

Loads Qwen3-32B-MLX-4bit, runs each (positive, negative) contrast pair through
the model with hooks attached at configurable layers, captures the residual
stream at the *last token position*, and computes the mean-difference vector
per layer.

Output: a npz file with shape (n_layers, hidden_dim) plus a JSON sidecar
manifest (model_id, layers, pair_set_hash, derived_at).

Usage:
    . .venv/bin/activate
    python tools/extract_vectors/spike/extract.py \\
        --model models/qwen3-32b-mlx-4bit \\
        --pairs tools/extract_vectors/spike/contrast_pairs.json \\
        --layers 30,40,50 \\
        --out data/vectors/spike_decisive_vs_hedging.npz
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

import mlx.core as mx
import numpy as np
from mlx_lm import load


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--model", default="models/qwen3-32b-mlx-4bit")
    p.add_argument("--pairs", default="tools/extract_vectors/spike/contrast_pairs.json")
    p.add_argument("--layers", default="20,30,40,50",
                   help="Comma-separated 0-based layer indices to extract from.")
    p.add_argument("--out", default="data/vectors/spike_decisive_vs_hedging.npz")
    p.add_argument("--max-pairs", type=int, default=0, help="Cap pairs (0 = all).")
    return p.parse_args()


def install_capture_hook(layer_module, layer_idx: int, captures: dict[int, list]):
    """Wrap `layer_module.__call__` via per-instance class-swap. Python looks
    up `__call__` on the *type*, so monkey-patching the instance attribute
    silently no-ops; we instead synthesize a subclass with the wrapped call
    and swap `layer_module.__class__` to it.

    Returns the original class for restoration.
    """
    original_class = type(layer_module)

    class _CaptureWrapped(original_class):
        pass

    parent_call = original_class.__call__

    def wrapped_call(self, x, *args, **kwargs):
        out = parent_call(self, x, *args, **kwargs)
        tensor = out[0] if isinstance(out, tuple) else out
        # MLX bf16 tensor → cast to f32 before numpy view (np buffer protocol
        # has no bf16 dtype). Last-token residual is the next-token-relevant
        # slice.
        last_f32 = tensor[:, -1, :].astype(mx.float32)
        mx.eval(last_f32)
        captures[layer_idx].append(np.array(last_f32, copy=True).squeeze(0))
        return out

    _CaptureWrapped.__call__ = wrapped_call
    layer_module.__class__ = _CaptureWrapped
    return original_class


def restore_hooks(layer_modules: dict[int, object], originals: dict[int, type]) -> None:
    for idx, layer in layer_modules.items():
        layer.__class__ = originals[idx]


def encode_prompt(tokenizer, prompt: str) -> mx.array:
    ids = tokenizer.encode(prompt)
    return mx.array(ids)[None, :]  # (1, seq)


def hash_pairs(pairs: list[dict]) -> str:
    h = hashlib.sha256()
    for p in pairs:
        h.update(p["positive"].encode("utf-8"))
        h.update(b"\x00")
        h.update(p["negative"].encode("utf-8"))
        h.update(b"\x01")
    return h.hexdigest()[:16]


def main() -> int:
    args = parse_args()
    layers = sorted(int(x) for x in args.layers.split(","))
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    print(f"loading {args.model}", flush=True)
    t0 = time.time()
    model, tokenizer = load(args.model)
    print(f"loaded in {time.time() - t0:.1f}s", flush=True)

    spec = json.loads(Path(args.pairs).read_text())
    pairs = spec["pairs"]
    if args.max_pairs > 0:
        pairs = pairs[: args.max_pairs]
    print(f"contrast pairs: {len(pairs)}; layers: {layers}", flush=True)

    # mlx-lm Qwen3 model surface: model.model.layers (list of TransformerBlock).
    # Match the layer indices against this list.
    base = getattr(model, "model", model)
    layer_modules_list = base.layers
    if any(idx >= len(layer_modules_list) for idx in layers):
        print(f"ERROR: requested layers {layers} but model only has {len(layer_modules_list)}",
              file=sys.stderr)
        return 2
    layer_modules = {idx: layer_modules_list[idx] for idx in layers}

    captures = {idx: [] for idx in layers}
    originals = {
        idx: install_capture_hook(layer_modules[idx], idx, captures)
        for idx in layers
    }

    try:
        for i, pair in enumerate(pairs):
            for label in ("positive", "negative"):
                # Tag the captures with a marker so we can split later.
                # Run a single forward pass; we only care about the residual
                # at the last token position.
                ids = encode_prompt(tokenizer, pair[label])
                _ = model(ids)
                mx.eval(_)  # force materialization
            if (i + 1) % 5 == 0 or i == len(pairs) - 1:
                print(f"  pair {i + 1}/{len(pairs)}", flush=True)
    finally:
        restore_hooks(layer_modules, originals)

    # captures[idx] now has 2 * n_pairs entries, alternating pos, neg.
    vectors = {}
    diagnostics = {}
    for idx in layers:
        arrs = np.stack(captures[idx])  # (2*n_pairs, hidden)
        pos = arrs[0::2]
        neg = arrs[1::2]
        diff = pos.mean(axis=0) - neg.mean(axis=0)
        vectors[f"L{idx}"] = diff.astype(np.float32)
        diagnostics[f"L{idx}"] = {
            "pos_mean_norm": float(np.linalg.norm(pos.mean(axis=0))),
            "neg_mean_norm": float(np.linalg.norm(neg.mean(axis=0))),
            "diff_norm": float(np.linalg.norm(diff)),
            "pos_var_avg": float(pos.var(axis=0).mean()),
            "neg_var_avg": float(neg.var(axis=0).mean()),
        }

    np.savez(out_path, **vectors)

    manifest_path = out_path.with_suffix(".manifest.json")
    manifest = {
        "model_id": "Qwen/Qwen3-32B",
        "model_quant": "mlx-4bit",
        "axis": spec["axis"],
        "layers": layers,
        "n_pairs": len(pairs),
        "contrast_pair_set_hash": hash_pairs(pairs),
        "embedder_version": "mlx-lm",
        "derived_at": datetime.now(timezone.utc).isoformat(),
        "diagnostics": diagnostics,
    }
    manifest_path.write_text(json.dumps(manifest, indent=2))

    print(f"\nwrote {out_path}")
    print(f"wrote {manifest_path}")
    print("\ndiagnostics:")
    for layer, d in diagnostics.items():
        print(f"  {layer}: diff_norm={d['diff_norm']:.3f} "
              f"pos_norm={d['pos_mean_norm']:.3f} neg_norm={d['neg_mean_norm']:.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
