# nanochat/tests/test_prepare.py
"""TDD tests for xvision_prepare.py — the fixed harness."""
from __future__ import annotations

import hashlib
import json
import math
import sqlite3
import struct
import tempfile
import textwrap
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Helpers — build a synthetic in-memory SQLite DB that mirrors the real schema
# ---------------------------------------------------------------------------

def _build_db(rows: list[dict[str, Any]]) -> sqlite3.Connection:
    """Create an in-memory SQLite DB with cycles/briefings/decisions/risk_outcomes.

    Each dict in `rows` must contain:
        cycle_id, open, high, low, close, volume,
        bars_forward (list of close prices for price_forward labeling),
        trader_direction ("LONG"|"SHORT"|"NEUTRAL"),
        pnl (float),
    """
    con = sqlite3.connect(":memory:")
    con.row_factory = sqlite3.Row
    con.executescript(textwrap.dedent("""
        CREATE TABLE cycles (
            cycle_id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z'
        );
        CREATE TABLE briefings (
            cycle_id TEXT PRIMARY KEY,
            ohlcv_json TEXT NOT NULL
        );
        CREATE TABLE decisions (
            cycle_id TEXT PRIMARY KEY,
            direction TEXT NOT NULL
        );
        CREATE TABLE risk_outcomes (
            cycle_id TEXT PRIMARY KEY,
            pnl REAL NOT NULL,
            bars_forward_json TEXT NOT NULL
        );
    """))
    for r in rows:
        con.execute("INSERT INTO cycles (cycle_id) VALUES (?)", (r["cycle_id"],))
        ohlcv = [[r["open"], r["high"], r["low"], r["close"], r["volume"]]]
        con.execute(
            "INSERT INTO briefings (cycle_id, ohlcv_json) VALUES (?, ?)",
            (r["cycle_id"], json.dumps(ohlcv)),
        )
        con.execute(
            "INSERT INTO decisions (cycle_id, direction) VALUES (?, ?)",
            (r["cycle_id"], r["trader_direction"]),
        )
        con.execute(
            "INSERT INTO risk_outcomes (cycle_id, pnl, bars_forward_json) VALUES (?, ?, ?)",
            (r["cycle_id"], r["pnl"], json.dumps(r["bars_forward"])),
        )
    con.commit()
    return con


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

ROWS_PRICE_FORWARD = [
    # id       open    high    low     close   vol  bars_forward (8 bars)            trader pnl
    {"cycle_id": "c001", "open": 100.0, "high": 101.0, "low": 99.0, "close": 100.0,
     "volume": 1000.0, "bars_forward": [100.0, 100.5, 101.0, 103.0, 103.5, 104.0, 104.5, 105.5],
     "trader_direction": "LONG", "pnl": 50.0},  # +5.5% → LONG (above threshold=0.02)

    {"cycle_id": "c002", "open": 200.0, "high": 201.0, "low": 199.0, "close": 200.0,
     "volume": 2000.0, "bars_forward": [200.0, 199.5, 199.0, 196.0, 195.5, 195.0, 194.5, 193.0],
     "trader_direction": "SHORT", "pnl": 70.0},  # -3.5% → SHORT (below -threshold)

    {"cycle_id": "c003", "open": 50.0, "high": 51.0, "low": 49.0, "close": 50.0,
     "volume": 500.0, "bars_forward": [50.0, 50.1, 50.0, 50.05, 49.9, 49.95, 50.1, 50.0],
     "trader_direction": "LONG", "pnl": -10.0},  # +0% → NEUTRAL (within threshold)

    {"cycle_id": "c004", "open": 150.0, "high": 152.0, "low": 149.0, "close": 150.0,
     "volume": 1500.0, "bars_forward": [150.0, 150.5, 151.0, 152.0, 153.0, 153.5, 154.0, 153.0],
     "trader_direction": "SHORT", "pnl": -20.0},  # +2.0% boundary: exactly at threshold=0.02 → NEUTRAL
]


# ---------------------------------------------------------------------------
# Tests — label_price_forward
# ---------------------------------------------------------------------------

def test_price_forward_long():
    from xvision_prepare import label_price_forward
    assert label_price_forward(entry_close=100.0, forward_closes=[105.5], threshold=0.02) == "LONG"


def test_price_forward_short():
    from xvision_prepare import label_price_forward
    assert label_price_forward(entry_close=200.0, forward_closes=[193.0], threshold=0.02) == "SHORT"


def test_price_forward_neutral_within_band():
    from xvision_prepare import label_price_forward
    # +0.0% change — well within ±2%
    assert label_price_forward(entry_close=50.0, forward_closes=[50.0], threshold=0.02) == "NEUTRAL"


def test_price_forward_neutral_exactly_at_threshold():
    """Boundary: return at exactly +threshold is NEUTRAL (not strictly above)."""
    from xvision_prepare import label_price_forward
    # +2.0% exactly → should be NEUTRAL (condition is > threshold, not >=)
    assert label_price_forward(entry_close=150.0, forward_closes=[153.0], threshold=0.02) == "NEUTRAL"


def test_price_forward_long_barely_above():
    """Just above threshold crosses to LONG."""
    from xvision_prepare import label_price_forward
    # +2.0001% → LONG
    assert label_price_forward(entry_close=100.0, forward_closes=[102.001], threshold=0.02) == "LONG"


def test_price_forward_short_barely_below():
    """Just below -threshold crosses to SHORT."""
    from xvision_prepare import label_price_forward
    assert label_price_forward(entry_close=100.0, forward_closes=[97.999], threshold=0.02) == "SHORT"


def test_price_forward_uses_last_bar_as_horizon():
    """When multiple forward bars given, uses the bar at window_bars index (last element)."""
    from xvision_prepare import label_price_forward
    # First bars show big gains but last bar (index -1) is flat
    assert label_price_forward(
        entry_close=100.0,
        forward_closes=[110.0, 115.0, 100.0],  # last bar = 100.0 → 0% → NEUTRAL
        threshold=0.02,
    ) == "NEUTRAL"


# ---------------------------------------------------------------------------
# Tests — label_outcome_imitation
# ---------------------------------------------------------------------------

def test_outcome_imitation_profitable_long():
    from xvision_prepare import label_outcome_imitation
    assert label_outcome_imitation(pnl=50.0, direction="LONG", min_pnl=0.0) == "LONG"


def test_outcome_imitation_profitable_short():
    from xvision_prepare import label_outcome_imitation
    assert label_outcome_imitation(pnl=70.0, direction="SHORT", min_pnl=0.0) == "SHORT"


def test_outcome_imitation_excludes_unprofitable():
    from xvision_prepare import label_outcome_imitation
    assert label_outcome_imitation(pnl=-10.0, direction="LONG", min_pnl=0.0) is None


def test_outcome_imitation_excludes_zero_pnl():
    """pnl == 0 is not strictly > 0 — excluded."""
    from xvision_prepare import label_outcome_imitation
    assert label_outcome_imitation(pnl=0.0, direction="LONG", min_pnl=0.0) is None


def test_outcome_imitation_custom_min_pnl():
    """min_pnl=10.0 should exclude a pnl=5.0 cycle."""
    from xvision_prepare import label_outcome_imitation
    assert label_outcome_imitation(pnl=5.0, direction="LONG", min_pnl=10.0) is None
    assert label_outcome_imitation(pnl=10.001, direction="LONG", min_pnl=10.0) == "LONG"


# ---------------------------------------------------------------------------
# Tests — compute_val_acc
# ---------------------------------------------------------------------------

def test_val_acc_hand_computed():
    """
    Hand-computed example:
      sample 0: predicted=LONG, label=LONG,  confidence=0.9 → correct=1, contrib=0.9
      sample 1: predicted=SHORT, label=LONG, confidence=0.8 → correct=0, contrib=0.0
      sample 2: predicted=LONG, label=LONG,  confidence=0.5 → correct=1, contrib=0.5
    val_acc = (0.9 + 0.0 + 0.5) / 3 = 0.4667 (to 4 dp)
    """
    from xvision_prepare import compute_val_acc

    predictions = [
        {"predicted": "LONG",  "label": "LONG",  "confidence": 0.9},
        {"predicted": "SHORT", "label": "LONG",  "confidence": 0.8},
        {"predicted": "LONG",  "label": "LONG",  "confidence": 0.5},
    ]
    result = compute_val_acc(predictions)
    assert abs(result - (0.9 + 0.0 + 0.5) / 3) < 1e-9


def test_val_acc_all_correct():
    from xvision_prepare import compute_val_acc
    predictions = [
        {"predicted": "LONG", "label": "LONG", "confidence": 1.0},
        {"predicted": "SHORT", "label": "SHORT", "confidence": 0.7},
    ]
    assert abs(compute_val_acc(predictions) - (1.0 + 0.7) / 2) < 1e-9


def test_val_acc_all_wrong():
    from xvision_prepare import compute_val_acc
    predictions = [
        {"predicted": "SHORT", "label": "LONG", "confidence": 0.9},
    ]
    assert compute_val_acc(predictions) == 0.0


# ---------------------------------------------------------------------------
# Tests — safetensors checkpoint writer + sha256 sidecar
# ---------------------------------------------------------------------------

def test_checkpoint_written_as_safetensors(tmp_path: Path):
    """write_checkpoint must produce a .safetensors file (not pickle)."""
    from xvision_prepare import write_checkpoint
    import torch

    weights = {"embed.weight": torch.zeros(10, 8)}
    out_dir = tmp_path / "ckpt"
    out_dir.mkdir()
    ckpt_path = write_checkpoint(weights=weights, out_dir=out_dir, name="model")

    assert ckpt_path.suffix == ".safetensors", "checkpoint must use safetensors format"
    assert ckpt_path.exists()

    # Verify it is a valid safetensors file (reads without error)
    from safetensors.torch import load_file
    loaded = load_file(str(ckpt_path))
    assert "embed.weight" in loaded


def test_checkpoint_sha256_sidecar_matches(tmp_path: Path):
    """sha256 sidecar must match the actual checkpoint bytes."""
    from xvision_prepare import write_checkpoint
    import torch

    weights = {"linear.weight": torch.ones(4, 4)}
    out_dir = tmp_path / "ckpt2"
    out_dir.mkdir()
    ckpt_path = write_checkpoint(weights=weights, out_dir=out_dir, name="model")

    sha_path = ckpt_path.with_suffix(".safetensors.sha256")
    assert sha_path.exists(), "sha256 sidecar must be written alongside checkpoint"

    expected_sha = hashlib.sha256(ckpt_path.read_bytes()).hexdigest()
    recorded_sha = sha_path.read_text().strip()
    assert recorded_sha == expected_sha, "sha256 sidecar does not match checkpoint bytes"


# ---------------------------------------------------------------------------
# Tests — insufficient data guard
# ---------------------------------------------------------------------------

def test_min_cycle_count_raises_below_threshold():
    from xvision_prepare import validate_cycle_count
    with pytest.raises(ValueError, match=r".*need.*500.*"):
        validate_cycle_count(available=120, required=500, strategy_name="TestStrat")


def test_min_cycle_count_passes_at_threshold():
    from xvision_prepare import validate_cycle_count
    # Should not raise at exactly min_cycle_count
    validate_cycle_count(available=500, required=500, strategy_name="TestStrat")


def test_min_cycle_count_passes_above_threshold():
    from xvision_prepare import validate_cycle_count
    validate_cycle_count(available=1000, required=500, strategy_name="TestStrat")
