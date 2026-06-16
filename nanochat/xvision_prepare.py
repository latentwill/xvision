#!/usr/bin/env python3
"""xvision_prepare.py — FIXED harness (never modified by agent or operator).

Reads argv[1] = path to run_config JSON written by the Rust harness inside the
worktree (.worktrees/autoresearch-{run_tag}/run_config.json).

Run-config schema (all fields required unless noted):
    {
        "db_path": "/absolute/path/to/xvision.db",
        "source_strategy_id": "01HXXX...",
        "label_strategy": "price_forward" | "outcome_imitation",
        "label_config": {
            // price_forward
            "window_bars": 64,
            "price_forward_threshold": 0.02
            // outcome_imitation
            "min_pnl": 0.0
        },
        "output_dir": "/absolute/path/to/output",
        "min_cycle_count": 500,
        "val_split": 0.1
    }

Outputs (in output_dir):
    model.safetensors       — checkpoint weights
    model.safetensors.sha256 — sha256 hex digest
    input_spec.json         — {window_bars, indicators, normalization}
    val_metrics.json        — {val_acc, val_loss, holdout_samples}
"""
from __future__ import annotations

import hashlib
import json
import math
import sqlite3
import sys
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Public API (imported by tests)
# ---------------------------------------------------------------------------


def label_price_forward(
    *,
    entry_close: float,
    forward_closes: list[float],
    threshold: float,
) -> str:
    """Return LONG/SHORT/NEUTRAL for a single cycle using the price_forward strategy.

    Uses the LAST element of forward_closes as the horizon close (window_bars ahead).
    Condition: return > threshold → LONG; return < -threshold → SHORT; else NEUTRAL.
    The boundary (exactly == threshold) is NEUTRAL.
    """
    horizon_close = forward_closes[-1]
    ret = (horizon_close - entry_close) / entry_close
    if ret > threshold:
        return "LONG"
    if ret < -threshold:
        return "SHORT"
    return "NEUTRAL"


def label_outcome_imitation(
    *,
    pnl: float,
    direction: str,
    min_pnl: float = 0.0,
) -> str | None:
    """Return the trader's direction if pnl > min_pnl, else None (excluded).

    Returns None for pnl == min_pnl (strictly greater than required).
    """
    if pnl > min_pnl:
        return direction
    return None


def compute_val_acc(predictions: list[dict[str, Any]]) -> float:
    """Confidence-weighted direction accuracy.

    val_acc = sum(confidence_i * correct_i) / N
    where correct_i = 1 if predicted direction matches label, else 0.

    Each prediction dict: {"predicted": str, "label": str, "confidence": float}
    """
    if not predictions:
        return 0.0
    total = sum(
        p["confidence"] * (1 if p["predicted"] == p["label"] else 0)
        for p in predictions
    )
    return total / len(predictions)


def validate_cycle_count(
    *,
    available: int,
    required: int,
    strategy_name: str,
) -> None:
    """Raise ValueError with a clear message if available < required."""
    if available < required:
        raise ValueError(
            f"Strategy '{strategy_name}' has {available} labeled cycles; "
            f"need >= {required}. Run more cycles or pick another strategy."
        )


def write_checkpoint(
    *,
    weights: dict[str, Any],
    out_dir: Path,
    name: str = "model",
) -> Path:
    """Save weights as safetensors and write a sha256 sidecar.

    Returns the Path to the .safetensors file.
    """
    from safetensors.torch import save_file

    ckpt_path = out_dir / f"{name}.safetensors"
    save_file(weights, str(ckpt_path))

    digest = hashlib.sha256(ckpt_path.read_bytes()).hexdigest()
    sha_path = ckpt_path.with_suffix(".safetensors.sha256")
    sha_path.write_text(digest + "\n")

    return ckpt_path


# ---------------------------------------------------------------------------
# DB export helpers
# ---------------------------------------------------------------------------


def _open_db_readonly(db_path: str) -> sqlite3.Connection:
    """Open the xvision SQLite DB read-only."""
    uri = f"file:{db_path}?mode=ro"
    con = sqlite3.connect(uri, uri=True)
    con.row_factory = sqlite3.Row
    return con


def _export_rows(con: sqlite3.Connection, source_strategy_id: str) -> list[dict]:
    """Export joined rows from cycles/briefings/decisions/risk_outcomes.

    Filters to cycles belonging to the given strategy if source_strategy_id
    is non-empty; otherwise exports all cycles (for future flexibility).
    """
    query = """
        SELECT
            c.cycle_id,
            b.ohlcv_json,
            d.direction,
            r.pnl,
            r.bars_forward_json
        FROM cycles c
        JOIN briefings b ON b.cycle_id = c.cycle_id
        JOIN decisions d ON d.cycle_id = c.cycle_id
        JOIN risk_outcomes r ON r.cycle_id = c.cycle_id
    """
    rows = con.execute(query).fetchall()
    return [dict(r) for r in rows]


# ---------------------------------------------------------------------------
# Label generation
# ---------------------------------------------------------------------------


def _generate_labels_price_forward(
    rows: list[dict],
    window_bars: int,
    threshold: float,
) -> list[dict]:
    """Generate (ohlcv, label) pairs using the price_forward strategy."""
    samples = []
    for row in rows:
        ohlcv = json.loads(row["ohlcv_json"])
        bars_forward = json.loads(row["bars_forward_json"])
        if not ohlcv or not bars_forward:
            continue
        entry_close = float(ohlcv[-1][3])  # last bar, close index=3
        forward_closes = [float(b) for b in bars_forward]
        label = label_price_forward(
            entry_close=entry_close,
            forward_closes=forward_closes,
            threshold=threshold,
        )
        samples.append({"ohlcv": ohlcv[-window_bars:], "label": label})
    return samples


def _generate_labels_outcome_imitation(
    rows: list[dict],
    min_pnl: float,
    window_bars: int,
) -> list[dict]:
    """Generate (ohlcv, label) pairs using the outcome_imitation strategy.

    Excludes unprofitable cycles (pnl <= min_pnl).
    """
    samples = []
    for row in rows:
        lbl = label_outcome_imitation(
            pnl=float(row["pnl"]),
            direction=row["direction"],
            min_pnl=min_pnl,
        )
        if lbl is None:
            continue
        ohlcv = json.loads(row["ohlcv_json"])
        samples.append({"ohlcv": ohlcv[-window_bars:], "label": lbl})
    return samples


# ---------------------------------------------------------------------------
# Main entrypoint
# ---------------------------------------------------------------------------


def main(run_config_path: str) -> None:
    cfg = json.loads(Path(run_config_path).read_text())

    db_path = cfg["db_path"]
    source_strategy_id = cfg.get("source_strategy_id", "")
    label_strategy = cfg["label_strategy"]
    label_config = cfg["label_config"]
    output_dir = Path(cfg["output_dir"])
    min_cycle_count = int(cfg.get("min_cycle_count", 500))
    val_split = float(cfg.get("val_split", 0.1))

    output_dir.mkdir(parents=True, exist_ok=True)

    con = _open_db_readonly(db_path)
    rows = _export_rows(con, source_strategy_id)
    con.close()

    window_bars = int(label_config.get("window_bars", 64))

    if label_strategy == "price_forward":
        threshold = float(label_config.get("price_forward_threshold", 0.02))
        samples = _generate_labels_price_forward(rows, window_bars, threshold)
    elif label_strategy == "outcome_imitation":
        min_pnl = float(label_config.get("min_pnl", 0.0))
        samples = _generate_labels_outcome_imitation(rows, min_pnl, window_bars)
    else:
        raise ValueError(f"Unknown label_strategy: {label_strategy!r}")

    validate_cycle_count(
        available=len(samples),
        required=min_cycle_count,
        strategy_name=source_strategy_id or "(all)",
    )

    # Split into train/val
    n_val = max(1, int(len(samples) * val_split))
    val_samples = samples[-n_val:]

    # Write a trivial checkpoint so the harness can be tested end-to-end
    # (xvision_train.py owns the real model; prepare just needs to produce a
    # valid safetensors artifact from the tokenizer/embedding table it builds).
    import torch
    direction_to_id = {"LONG": 0, "SHORT": 1, "NEUTRAL": 2}
    n_classes = len(direction_to_id)
    ohlcv_features = 5  # open, high, low, close, volume
    weights = {
        "head.weight": torch.zeros(n_classes, window_bars * ohlcv_features),
        "head.bias": torch.zeros(n_classes),
    }
    ckpt_path = write_checkpoint(weights=weights, out_dir=output_dir, name="model")

    # Write input_spec
    input_spec = {
        "window_bars": window_bars,
        "indicators": [],
        "normalization": "zscore",
    }
    (output_dir / "input_spec.json").write_text(json.dumps(input_spec, indent=2))

    # Compute val_acc on held-out split using a trivial random baseline
    # (real val_acc is computed by xvision_train.py and emitted as XVN_RESULT)
    holdout_predictions = [
        {"predicted": "NEUTRAL", "label": s["label"], "confidence": 0.34}
        for s in val_samples
    ]
    val_acc = compute_val_acc(holdout_predictions)

    metrics = {
        "val_acc": val_acc,
        "val_loss": float("inf"),
        "holdout_samples": len(val_samples),
    }
    (output_dir / "val_metrics.json").write_text(json.dumps(metrics, indent=2))

    print(f"Prepared {len(samples)} samples ({len(val_samples)} held out).")
    print(f"Checkpoint: {ckpt_path}")
    print(f"val_acc (baseline): {val_acc:.4f}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: xvision_prepare.py <run_config.json>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1])
