#!/usr/bin/env python3
"""Validate and attach xvision inline JSON strategy filters.

The dashboard route for inline filters is ``PATCH /api/strategy/:id`` with a
``filter`` field. There is no standalone ``PUT /api/strategy/:id/filter`` route.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from xvn_api import XvnApi, XvnApiError, print_json

CROSS_OPS = {"crosses_above", "crosses_below"}
KNOWN_OPS = CROSS_OPS | {"above", "below", "equals"}


def iter_conditions(node):
    if isinstance(node, dict) and ("all" in node or "any" in node):
        for key in ("all", "any"):
            for child in node.get(key, []) or []:
                yield from iter_conditions(child)
    else:
        yield node


def validate_filter(f: dict) -> list[str]:
    errors = []
    for key in ["display_name", "asset_scope", "timeframe", "conditions"]:
        if key not in f:
            errors.append(f"missing filter.{key}")
    if not isinstance(f.get("asset_scope"), list):
        errors.append("filter.asset_scope must be a list")
    for idx, cond in enumerate(iter_conditions(f.get("conditions", {}))):
        if not isinstance(cond, dict):
            errors.append(f"condition {idx} must be an object")
            continue
        for key in ["lhs", "op", "rhs"]:
            if key not in cond:
                errors.append(f"condition {idx} missing {key}")
        op = cond.get("op")
        if op and op not in KNOWN_OPS:
            errors.append(f"condition {idx} unknown op {op!r}")
        if op in CROSS_OPS and isinstance(cond.get("rhs"), (int, float)):
            errors.append(f"condition {idx} {op} requires indicator RHS, not numeric")
    return errors


def load_payload(path: str) -> dict:
    raw = json.loads(Path(path).read_text(encoding="utf-8"))
    return raw if "filter" in raw else {"filter": raw}


def main(argv=None) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=None)
    sub = p.add_subparsers(dest="cmd", required=True)
    validate_cmd = sub.add_parser("validate")
    validate_cmd.add_argument("filter_json")
    attach_cmd = sub.add_parser("attach")
    attach_cmd.add_argument("strategy_id")
    attach_cmd.add_argument("filter_json")
    attach_cmd.add_argument("--dry-run", action="store_true")
    attach_cmd.add_argument("--yes", action="store_true", help="required to mutate the dashboard")
    args = p.parse_args(argv)

    payload = load_payload(args.filter_json)
    filter_body = payload.get("filter", {})
    errors = validate_filter(filter_body)
    if args.cmd == "validate":
        print_json({"ok": not errors, "errors": errors, "payload": payload})
        return 0 if not errors else 2
    if errors:
        print_json({"ok": False, "errors": errors})
        return 2
    request_body = {"filter": filter_body}
    if args.dry_run or not args.yes:
        print_json(
            {
                "dry_run": True,
                "would_patch": f"/api/strategy/{args.strategy_id}",
                "payload": request_body,
                "mutation_requires": "--yes",
            }
        )
        return 0
    try:
        resp = XvnApi(args.url).patch(f"/api/strategy/{args.strategy_id}", request_body)
        print_json({"status": resp.status, "payload": resp.payload})
        return 0
    except XvnApiError as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
