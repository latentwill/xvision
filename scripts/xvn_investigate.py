#!/usr/bin/env python3
"""Diagnose xvision strategy/filter/eval/provider issues with evidence."""
from __future__ import annotations

import argparse
import os

from xvn_api import XvnApi, print_json
from xvn_eval_harness import summarize_export

DEFAULT_URL = os.environ.get("XVN_BASE_URL", "https://xvn.tail2bb69.ts.net")


def try_api(api, method, path):
    try:
        return {"ok": True, "response": getattr(api, method)(path).payload}
    except Exception as exc:
        return {"ok": False, "error": str(exc)}


def diagnose(api, strategy_id=None, run_id=None, scenario_id=None):
    out = {"base_url": api.base_url, "findings": []}
    if strategy_id:
        strat = try_api(api, "get", f"/api/strategy/{strategy_id}")
        out["strategy_api"] = strat
        if not strat["ok"]:
            out["findings"].append(
                {
                    "severity": "high",
                    "finding": "strategy detail API failed",
                    "evidence": strat.get("error"),
                }
            )
    if scenario_id:
        out["scenario_api"] = try_api(api, "get", f"/api/scenarios/{scenario_id}")
    if run_id:
        run = try_api(api, "get", f"/api/eval/runs/{run_id}")
        out["run_api"] = run
        exp = try_api(api, "get", f"/api/eval/runs/{run_id}/export")
        out["export_api_ok"] = exp["ok"]
        if exp["ok"]:
            summary = summarize_export(exp["response"])
            out["export_summary"] = summary
            if not summary["export_integrity"]["strategy_present"]:
                out["findings"].append(
                    {"severity": "medium", "finding": "eval export has null strategy"}
                )
            if summary["export_integrity"]["agents_count"] == 0:
                out["findings"].append(
                    {"severity": "medium", "finding": "eval export has empty agents list"}
                )
            if not summary.get("filter_summaries"):
                out["findings"].append(
                    {"severity": "medium", "finding": "no filter_summaries in eval export"}
                )
        else:
            out["findings"].append(
                {"severity": "high", "finding": "eval export failed", "evidence": exp.get("error")}
            )
    return out


def main(argv=None):
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=DEFAULT_URL)
    p.add_argument("--strategy")
    p.add_argument("--run")
    p.add_argument("--scenario")
    args = p.parse_args(argv)
    print_json(diagnose(XvnApi(args.url), args.strategy, args.run, args.scenario))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
