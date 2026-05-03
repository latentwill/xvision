"""Phase 4.1 — Contrastive dataset loader and templated-pair generator.

Each YAML spec defines one disposition axis.  ``load_spec`` reads it;
``generate_pairs`` applies the Mitra template (§4.1) over the Cartesian
product of assets × regimes × behavior-pairs and caps at ``target_pairs``.

Hash contract
-------------
``pair_set_hash`` produces a stable 64-char SHA-256 hex over
``json.dumps(pairs, sort_keys=True)`` so the manifest contract's
``contrast_pair_set_hash`` field is reproducible across runs and machines.

CLI
---
    python -m tools.extract_vectors.datasets --spec <yaml> --hash
    python -m tools.extract_vectors.datasets --spec <yaml> --out <json>
Or from within the directory:
    python datasets.py --spec specs/conviction.yaml --hash
"""

from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import sys
from pathlib import Path
from typing import Any

import yaml


TEMPLATE = (
    "An analyst evaluates {asset} during {regime}. "
    "The analyst's view: {behavior}."
)


# ---------------------------------------------------------------------------
# Spec loading
# ---------------------------------------------------------------------------

def load_spec(path: str | Path) -> dict[str, Any]:
    """Load and return the raw YAML spec dict."""
    data = yaml.safe_load(Path(path).read_text())
    required = {"axis_name", "assets", "regimes", "positive_behaviors", "negative_behaviors"}
    missing = required - data.keys()
    if missing:
        raise ValueError(f"Spec is missing required keys: {missing}")
    return data


# ---------------------------------------------------------------------------
# Pair generation
# ---------------------------------------------------------------------------

def generate_pairs(spec: dict[str, Any]) -> list[dict[str, str]]:
    """Return the templated contrast pairs for *spec*.

    Algorithm:
    1. Form the Cartesian product: asset × regime × (pos_behavior, neg_behavior).
    2. Each triplet produces one ``{positive, negative}`` pair.
    3. The pair list is deterministically ordered (product order of the lists
       in the spec), then capped at ``target_pairs`` if specified.

    The cap truncates after deterministic ordering so that hashes are stable
    regardless of how many times the function is called with the same spec.
    """
    assets: list[str] = spec["assets"]
    regimes: list[str] = spec["regimes"]
    pos_behaviors: list[str] = spec["positive_behaviors"]
    neg_behaviors: list[str] = spec["negative_behaviors"]
    target: int | None = spec.get("target_pairs")

    # Zip behaviors into (pos, neg) pairs: first n_min of each side.
    n_behaviors = min(len(pos_behaviors), len(neg_behaviors))
    behavior_pairs = list(zip(pos_behaviors[:n_behaviors], neg_behaviors[:n_behaviors]))

    pairs: list[dict[str, str]] = []
    for asset, regime, (pos_b, neg_b) in itertools.product(assets, regimes, behavior_pairs):
        pairs.append(
            {
                "positive": TEMPLATE.format(asset=asset, regime=regime, behavior=pos_b),
                "negative": TEMPLATE.format(asset=asset, regime=regime, behavior=neg_b),
            }
        )

    if target is not None:
        pairs = pairs[:target]

    return pairs


# ---------------------------------------------------------------------------
# Hash
# ---------------------------------------------------------------------------

def pair_set_hash(pairs: list[dict]) -> str:
    """Stable 64-char SHA-256 hex over ``json.dumps(pairs, sort_keys=True)``.

    Matches the ``contrast_pair_set_hash`` field in the Rust manifest contract.
    """
    serialized = json.dumps(pairs, sort_keys=True, ensure_ascii=False)
    return hashlib.sha256(serialized.encode("utf-8")).hexdigest()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Generate contrastive dataset pairs from a disposition spec."
    )
    p.add_argument("--spec", required=True, help="Path to the axis YAML spec.")
    group = p.add_mutually_exclusive_group(required=True)
    group.add_argument("--hash", action="store_true", help="Print the 64-char pair-set hash.")
    group.add_argument("--out", metavar="JSON", help="Write the materialized pair set to this JSON file.")
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    spec = load_spec(args.spec)
    pairs = generate_pairs(spec)

    if args.hash:
        print(pair_set_hash(pairs))
    else:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "axis_name": spec["axis_name"],
            "n_pairs": len(pairs),
            "pairs": pairs,
        }
        out_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False))
        print(f"Wrote {len(pairs)} pairs to {out_path}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    sys.exit(main())
