# nanochat/tests/test_train.py
"""TDD tests for xvision_train.py — the agent-editable baseline.

Tests are deliberately fast (tiny synthetic tensors, 1–2 training steps,
CPU only) so they run in CI without a GPU.
"""
from __future__ import annotations

import json
import subprocess
import sys
import textwrap
from pathlib import Path
from unittest import mock

import pytest
import torch


# ---------------------------------------------------------------------------
# Device-selection helper unit test
# ---------------------------------------------------------------------------

def test_device_select_mps_when_available():
    """select_device() returns 'mps' when MPS is available."""
    with (
        mock.patch("torch.backends.mps.is_available", return_value=True),
        mock.patch("torch.cuda.is_available", return_value=False),
    ):
        from xvision_train import select_device
        assert select_device() == "mps"


def test_device_select_cuda_when_mps_unavailable():
    """select_device() returns 'cuda' when MPS is not available but CUDA is."""
    with (
        mock.patch("torch.backends.mps.is_available", return_value=False),
        mock.patch("torch.cuda.is_available", return_value=True),
    ):
        from xvision_train import select_device
        assert select_device() == "cuda"


def test_device_select_cpu_fallback():
    """select_device() falls back to 'cpu' when neither MPS nor CUDA is available."""
    with (
        mock.patch("torch.backends.mps.is_available", return_value=False),
        mock.patch("torch.cuda.is_available", return_value=False),
    ):
        from xvision_train import select_device
        assert select_device() == "cpu"


# ---------------------------------------------------------------------------
# Smoke test — end-to-end subprocess run
# ---------------------------------------------------------------------------

_SMOKE_CONFIG = {
    "db_path": ":memory:",  # not used by train; prepare handles DB
    "source_strategy_id": "smoke",
    "label_strategy": "price_forward",
    "label_config": {"window_bars": 4, "price_forward_threshold": 0.02},
    "output_dir": "",       # filled in per test
    "min_cycle_count": 2,
    "val_split": 0.5,
    "train_wall_clock_sec": 30,
    "max_train_steps": 2,   # fast override for CI
}


def test_train_emits_xvn_result_line(tmp_path: Path):
    """xvision_train.py must print a parseable XVN_RESULT line as its last output line."""
    # Write minimal synthetic data: 4 OHLCV samples, window=4 bars
    data_dir = tmp_path / "data"
    data_dir.mkdir()

    # Write a tiny synthetic batch directly as input tensors
    # (bypassing xvision_prepare.py — the train script accepts a --data-dir with
    #  pre-built tensors, or reads from the run config's output_dir)
    n_samples = 4
    window_bars = 4
    ohlcv_features = 5
    X = torch.randn(n_samples, window_bars * ohlcv_features)
    # Labels: 0=LONG, 1=SHORT, 2=NEUTRAL
    y = torch.tensor([0, 1, 2, 0], dtype=torch.long)
    torch.save({"X": X, "y": y}, data_dir / "train.pt")
    torch.save({"X": X, "y": y}, data_dir / "val.pt")

    cfg = {**_SMOKE_CONFIG, "output_dir": str(data_dir)}
    run_config_path = tmp_path / "run_config.json"
    run_config_path.write_text(json.dumps(cfg))

    train_script = Path(__file__).parent.parent / "xvision_train.py"
    result = subprocess.run(
        [sys.executable, str(train_script), str(run_config_path)],
        capture_output=True,
        text=True,
        timeout=60,
    )

    assert result.returncode == 0, (
        f"xvision_train.py exited {result.returncode}.\n"
        f"stdout: {result.stdout}\nstderr: {result.stderr}"
    )

    # The LAST non-empty line must be parseable XVN_RESULT
    lines = [l.strip() for l in result.stdout.splitlines() if l.strip()]
    assert lines, "xvision_train.py produced no output"
    last_line = lines[-1]
    assert last_line.startswith("XVN_RESULT "), (
        f"Last output line must start with 'XVN_RESULT ', got: {last_line!r}"
    )

    payload_str = last_line[len("XVN_RESULT "):]
    try:
        payload = json.loads(payload_str)
    except json.JSONDecodeError as exc:
        pytest.fail(f"XVN_RESULT payload is not valid JSON: {exc}\nLine: {last_line!r}")

    assert "val_acc" in payload, f"XVN_RESULT must contain 'val_acc', got: {payload}"
    assert "val_loss" in payload, f"XVN_RESULT must contain 'val_loss', got: {payload}"
    assert isinstance(payload["val_acc"], (int, float)), "val_acc must be numeric"
    assert isinstance(payload["val_loss"], (int, float)), "val_loss must be numeric"
    assert 0.0 <= payload["val_acc"] <= 1.0, f"val_acc out of range: {payload['val_acc']}"
