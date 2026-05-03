"""Smoke tests for datasets.py — Phase 4.1.

Run:
    pytest tools/extract_vectors/test_datasets.py -v

Tests:
    1. Conviction set hash is identical across two independent calls (deterministic).
    2. Conviction set produces >= 200 pairs.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Allow running from repo root or from this directory
_THIS_DIR = Path(__file__).parent
sys.path.insert(0, str(_THIS_DIR))

from datasets import generate_pairs, load_spec, pair_set_hash  # noqa: E402

CONVICTION_SPEC = _THIS_DIR / "specs" / "conviction.yaml"


@pytest.fixture(scope="module")
def conviction_spec():
    return load_spec(CONVICTION_SPEC)


@pytest.fixture(scope="module")
def conviction_pairs(conviction_spec):
    return generate_pairs(conviction_spec)


def test_conviction_hash_is_deterministic(conviction_spec):
    """Hash must be identical across two independent calls with the same spec."""
    pairs_a = generate_pairs(conviction_spec)
    pairs_b = generate_pairs(conviction_spec)
    assert pair_set_hash(pairs_a) == pair_set_hash(pairs_b), (
        "Pair-set hash is not deterministic — check ordering in generate_pairs()"
    )


def test_conviction_pair_count(conviction_pairs):
    """Must produce at least 200 pairs (Mitra lower bound for the active axis)."""
    assert len(conviction_pairs) >= 200, (
        f"Expected >= 200 conviction pairs, got {len(conviction_pairs)}"
    )


def test_conviction_pairs_have_positive_and_negative(conviction_pairs):
    """Every pair must have non-empty 'positive' and 'negative' keys."""
    for i, pair in enumerate(conviction_pairs):
        assert "positive" in pair and pair["positive"], f"Pair {i} missing 'positive'"
        assert "negative" in pair and pair["negative"], f"Pair {i} missing 'negative'"


def test_conviction_hash_is_64_chars(conviction_pairs):
    """Contract requires a 64-char SHA-256 hex string."""
    h = pair_set_hash(conviction_pairs)
    assert len(h) == 64, f"Expected 64-char hash, got {len(h)}: {h!r}"


def test_conviction_pairs_differ(conviction_pairs):
    """Positive and negative prompts must differ for every pair."""
    for i, pair in enumerate(conviction_pairs):
        assert pair["positive"] != pair["negative"], (
            f"Pair {i}: positive == negative — template bug?"
        )
