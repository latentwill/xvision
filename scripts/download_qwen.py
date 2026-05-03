"""Download Qwen3-32B in two forms:
  1. MLX 4-bit checkpoint (`mlx-community/Qwen3-32B-4bit`, ~18 GB) — for repeng-style
     vector extraction via MLX hooks (Phase 0.3 spike + Phase 4.x extraction).
  2. GGUF Q4_K_M (`bartowski/Qwen_Qwen3-32B-GGUF`, ~20 GB) — for candle Q4 inference
     with steering hooks installed (Phase 0.2 smoke test + Phase 3+ runtime).

Both are public — no HF token needed. Token speeds up rate limits if set in
HUGGING_FACE_HUB_TOKEN env var.

Usage:
    . .venv/bin/activate && python scripts/download_qwen.py
"""

from __future__ import annotations
import os
import sys
from pathlib import Path

from huggingface_hub import snapshot_download, hf_hub_download

MODELS_DIR = Path(__file__).resolve().parent.parent / "models"

MLX_REPO = "mlx-community/Qwen3-32B-4bit"
MLX_DIR = "qwen3-32b-mlx-4bit"

GGUF_REPO = "bartowski/Qwen_Qwen3-32B-GGUF"
GGUF_FILE = "Qwen_Qwen3-32B-Q4_K_M.gguf"
GGUF_DIR = "qwen3-32b-q4-gguf"


def main() -> int:
    MODELS_DIR.mkdir(exist_ok=True)
    token = os.environ.get("HUGGING_FACE_HUB_TOKEN")

    print(f"[1/2] Downloading {MLX_REPO} (MLX 4-bit, ~18 GB) → models/{MLX_DIR}/")
    snapshot_download(
        repo_id=MLX_REPO,
        local_dir=str(MODELS_DIR / MLX_DIR),
        token=token,
        ignore_patterns=["*.msgpack", "*.h5", "*.ot", "*.gguf", "*flax*", "*.onnx"],
    )

    print(f"[2/2] Downloading {GGUF_REPO}/{GGUF_FILE} (Q4_K_M GGUF, ~20 GB) → models/{GGUF_DIR}/")
    hf_hub_download(
        repo_id=GGUF_REPO,
        filename=GGUF_FILE,
        local_dir=str(MODELS_DIR / GGUF_DIR),
        token=token,
    )

    print("\nDone. Layout:")
    for p in sorted(MODELS_DIR.rglob("*")):
        if p.is_file() and p.stat().st_size > 1024 * 1024:
            print(f"  {p.relative_to(MODELS_DIR.parent)}  ({p.stat().st_size / 1024 / 1024:.1f} MiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
