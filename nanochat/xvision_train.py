#!/usr/bin/env python3
"""xvision_train.py — AGENT-EDITABLE baseline model.

This file is freely modified by the autoresearcher agent between experiments.
The operator may also edit it directly (see xvision_program.md for instructions).

Contract with the Rust harness (DO NOT REMOVE):
  - The LAST line printed to stdout must be:
        XVN_RESULT {"val_acc": <float>, "val_loss": <float>}
  - Any other stdout/stderr is streamed to the UI live feed and ignored by the parser.
  - Exit 0 on success; non-zero on crash (the harness records val_acc = NULL).

Reads argv[1] = path to run_config.json (written by the Rust harness).
Reads pre-built tensors from run_config["output_dir"]/train.pt and val.pt
(produced by xvision_prepare.py, or synthetic tensors in tests).

Run-config keys used by this script:
    output_dir: where train.pt and val.pt live (and where we write model.safetensors)
    train_wall_clock_sec: wall-clock training budget (seconds), default 300
    max_train_steps: (optional) override for CI/smoke: max gradient steps

Baseline architecture (v0): linear head over flattened OHLCV window.
The agent should replace this with a GPT2-scale model.
"""
from __future__ import annotations

import json
import sys
import time
from pathlib import Path

import torch
import torch.nn as nn
import torch.nn.functional as F


# ---------------------------------------------------------------------------
# Public helper — imported by tests
# ---------------------------------------------------------------------------


def select_device() -> str:
    """Return 'mps', 'cuda', or 'cpu' — whichever is available first."""
    if torch.backends.mps.is_available():
        return "mps"
    if torch.cuda.is_available():
        return "cuda"
    return "cpu"


# ---------------------------------------------------------------------------
# Baseline model (agent-editable)
# ---------------------------------------------------------------------------


class NanochatBaseline(nn.Module):
    """Minimal linear classifier over a flattened OHLCV window.

    Input:  (batch, window_bars * ohlcv_features)
    Output: (batch, 3)  — logits for [LONG, SHORT, NEUTRAL]
    """

    def __init__(self, input_dim: int, n_classes: int = 3) -> None:
        super().__init__()
        self.linear = nn.Linear(input_dim, n_classes)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.linear(x)


# ---------------------------------------------------------------------------
# Training loop
# ---------------------------------------------------------------------------


def _load_tensors(path: Path) -> tuple[torch.Tensor, torch.Tensor]:
    data = torch.load(str(path), weights_only=True)
    return data["X"], data["y"]


def _compute_val_acc(
    model: nn.Module,
    X_val: torch.Tensor,
    y_val: torch.Tensor,
    device: str,
) -> tuple[float, float]:
    """Confidence-weighted direction accuracy + cross-entropy loss on the val set."""
    model.eval()
    with torch.no_grad():
        X_d = X_val.to(device)
        y_d = y_val.to(device)
        logits = model(X_d)
        loss = F.cross_entropy(logits, y_d).item()
        probs = torch.softmax(logits, dim=-1)
        predicted = probs.argmax(dim=-1)
        confidence = probs.max(dim=-1).values
        correct = (predicted == y_d).float()
        val_acc = (confidence * correct).mean().item()
    model.train()
    return float(val_acc), float(loss)


def train(run_config: dict) -> dict:
    """Run the training loop. Returns {val_acc, val_loss}."""
    output_dir = Path(run_config["output_dir"])
    wall_clock_sec = float(run_config.get("train_wall_clock_sec", 300))
    max_steps = run_config.get("max_train_steps")  # None means wall-clock only

    device = select_device()
    print(f"Device: {device}")

    X_train, y_train = _load_tensors(output_dir / "train.pt")
    X_val, y_val = _load_tensors(output_dir / "val.pt")

    input_dim = X_train.shape[1]
    model = NanochatBaseline(input_dim=input_dim).to(device)
    optimizer = torch.optim.Adam(model.parameters(), lr=1e-3)

    deadline = time.monotonic() + wall_clock_sec
    step = 0
    batch_size = min(32, len(X_train))

    X_d = X_train.to(device)
    y_d = y_train.to(device)

    model.train()
    while True:
        if time.monotonic() >= deadline:
            print(f"Wall-clock budget ({wall_clock_sec}s) reached after {step} steps.")
            break
        if max_steps is not None and step >= int(max_steps):
            break

        # Mini-batch (simple random sampling with replacement)
        idx = torch.randint(0, len(X_d), (batch_size,))
        logits = model(X_d[idx])
        loss = F.cross_entropy(logits, y_d[idx])
        optimizer.zero_grad()
        loss.backward()
        optimizer.step()
        step += 1

        if step % 10 == 0:
            print(f"step={step} loss={loss.item():.4f}")

    val_acc, val_loss = _compute_val_acc(model, X_val, y_val, device)

    # Save checkpoint
    from safetensors.torch import save_file
    weights = {k: v.cpu() for k, v in model.state_dict().items()}
    save_file(weights, str(output_dir / "model.safetensors"))
    print(f"Checkpoint written: {output_dir / 'model.safetensors'}")

    return {"val_acc": val_acc, "val_loss": val_loss}


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------


def main(run_config_path: str) -> None:
    cfg = json.loads(Path(run_config_path).read_text())
    metrics = train(cfg)
    # MUST be the last line of stdout — harness parses this.
    print(f"XVN_RESULT {json.dumps(metrics)}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: xvision_train.py <run_config.json>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1])
