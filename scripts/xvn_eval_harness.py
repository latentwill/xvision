#!/usr/bin/env python3
"""Export and summarize xvision evals consistently."""
from __future__ import annotations

import argparse
import collections
import os

from xvn_api import XvnApi, print_json

DEFAULT_BASE_URL = os.environ.get("XVN_BASE_URL", "https://xvn.tail2bb69.ts.net")


def export_run(api: XvnApi, run_id: str) -> dict:
    return api.get(f"/api/eval/runs/{run_id}/export").payload


def action_for_decision(decision: dict) -> str | None:
    nested = decision.get("decision")
    if isinstance(nested, dict):
        return nested.get("action")
    return decision.get("action")


def summarize_export(data: dict) -> dict:
    run = data.get("run") or {}
    decisions = data.get("decisions") or []
    actions = collections.Counter(action_for_decision(d) for d in decisions)
    actions.pop(None, None)
    return {
        "run_id": run.get("id"),
        "status": run.get("status"),
        "strategy_id": run.get("strategy_id") or run.get("agent_id"),
        "scenario_id": run.get("scenario_id"),
        "metrics": data.get("metrics") or run.get("metrics"),
        "decision_count": len(decisions),
        "actions": dict(actions),
        "filter_summaries": data.get("filter_summaries") or [],
        "export_integrity": {
            "strategy_present": data.get("strategy") is not None,
            "agents_count": len(data.get("agents") or []),
        },
        "errors": data.get("errors") or [],
        "provider_diagnostics": data.get("provider_diagnostics"),
    }


def main(argv=None) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=DEFAULT_BASE_URL)
    sub = p.add_subparsers(dest="cmd", required=True)
    export_summary = sub.add_parser("export-summary")
    export_summary.add_argument("run_id")
    args = p.parse_args(argv)
    api = XvnApi(args.url)
    if args.cmd == "export-summary":
        print_json(summarize_export(export_run(api, args.run_id)))
        return 0
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
