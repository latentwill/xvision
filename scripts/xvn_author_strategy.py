#!/usr/bin/env python3
"""Create a complete xvision strategy package from a JSON spec.

Prefer local ``xvn strategy new`` when running on the node. This helper uses the
dashboard API for explicit remote authoring because the remote CLI intentionally
denies mutating strategy commands.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from xvn_api import XvnApi, XvnApiError, print_json
from xvn_filter_lab import validate_filter

MINIMAL_STRATEGY = ["name"]


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def validate_spec(spec: dict) -> list[str]:
    errors = [f"missing {key}" for key in MINIMAL_STRATEGY if key not in spec]
    if "filter" in spec:
        errors += [f"filter: {e}" for e in validate_filter(spec["filter"])]
    return errors


def planned_steps() -> list[str]:
    return [
        "POST /api/strategies",
        "PATCH /api/strategy/:id",
        "POST /api/agents",
        "POST /api/strategy/:id/agents",
        "POST /api/strategy/:id/validate",
    ]


def create_package(api: XvnApi, spec: dict, dry_run: bool = False) -> dict:
    if dry_run:
        return {"dry_run": True, "planned_steps": planned_steps(), "spec": spec}

    result = {"steps": []}
    create_body = {"name": spec["name"]}
    if spec.get("creator") is not None:
        create_body["creator"] = spec["creator"]
    create = api.post("/api/strategies", create_body).payload
    sid = create["id"]
    result["strategy_id"] = sid
    result["steps"].append({"create_strategy": create})

    patch = dict(spec.get("manifest_patch") or {})
    if filt := spec.get("filter"):
        patch["filter"] = filt
    if patch:
        result["steps"].append({"patch_strategy": api.patch(f"/api/strategy/{sid}", patch).payload})

    if agent := spec.get("agent"):
        created_agent = api.post("/api/agents", agent).payload
        result["agent"] = created_agent
        aid = created_agent.get("agent_id") or created_agent.get("id")
        if aid:
            attach_body = {"agent_id": aid, "role": agent.get("role", "trader")}
            result["steps"].append(
                {"attach_agent": api.post(f"/api/strategy/{sid}/agents", attach_body).payload}
            )

    try:
        result["validation"] = api.post(f"/api/strategy/{sid}/validate", {}).payload
    except Exception as exc:
        result["validation_error"] = str(exc)
    return result


def main(argv=None) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=None)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--yes", action="store_true", help="required to mutate the dashboard")
    p.add_argument("spec_json")
    args = p.parse_args(argv)

    spec = load(args.spec_json)
    errors = validate_spec(spec)
    if errors:
        print_json({"ok": False, "errors": errors})
        return 2

    try:
        print_json(create_package(XvnApi(args.url), spec, args.dry_run or not args.yes))
        return 0
    except XvnApiError as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
