#!/usr/bin/env python3
"""Create/validate xvision scenario specs via the dashboard API.

Prefer local ``xvn scenario create`` on the node. This helper is for agents that
need to validate or submit full dashboard ``CreateScenarioRequest`` JSON over
Tailscale.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from xvn_api import XvnApi, XvnApiError, print_json

REQUIRED = [
    "display_name",
    "description",
    "asset_class",
    "asset",
    "quote_currency",
    "time_window",
    "capital",
    "granularity",
    "timezone",
    "calendar",
    "venue",
    "data_source",
    "replay_mode",
    "tags",
    "source",
]


def validate_spec(spec: dict) -> list[str]:
    errors = [f"missing {key}" for key in REQUIRED if key not in spec]
    if "tags" in spec and not isinstance(spec["tags"], list):
        errors.append("tags must be a list")
    if "asset" in spec and not isinstance(spec["asset"], list):
        errors.append("asset must be a list")
    return errors


def main(argv=None) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=None)
    sub = p.add_subparsers(dest="cmd", required=True)
    validate_cmd = sub.add_parser("validate")
    validate_cmd.add_argument("spec_json")
    create_cmd = sub.add_parser("create")
    create_cmd.add_argument("spec_json")
    create_cmd.add_argument("--dry-run", action="store_true")
    create_cmd.add_argument("--yes", action="store_true", help="required to mutate the dashboard")
    inspect_cmd = sub.add_parser("inspect")
    inspect_cmd.add_argument("scenario_id")
    args = p.parse_args(argv)
    api = XvnApi(args.url)

    if args.cmd in {"validate", "create"}:
        spec = json.loads(Path(args.spec_json).read_text(encoding="utf-8"))
        errors = validate_spec(spec)
        if args.cmd == "validate" or errors or args.dry_run or not args.yes:
            print_json(
                {
                    "ok": not errors,
                    "errors": errors,
                    "spec": spec,
                    "dry_run": args.cmd == "create",
                    "mutation_requires": "--yes" if args.cmd == "create" else None,
                }
            )
            return 0 if not errors else 2
        try:
            resp = api.post("/api/scenarios", spec)
            print_json({"status": resp.status, "payload": resp.payload})
            return 0
        except XvnApiError as exc:
            print(str(exc), file=sys.stderr)
            return 1

    if args.cmd == "inspect":
        print_json(api.get(f"/api/scenarios/{args.scenario_id}").payload)
        return 0
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
