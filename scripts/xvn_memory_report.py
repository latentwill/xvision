#!/usr/bin/env python3
"""Read xvision memory and propose reviewable durable memory notes.

This script does not write memory. It produces proposals for a human/operator to
review before anything durable is saved.
"""
from __future__ import annotations

import argparse
import os

from xvn_api import XvnApi, print_json

DEFAULT_URL = os.environ.get("XVN_BASE_URL", "https://xvn.tail2bb69.ts.net")


def main(argv=None):
    p = argparse.ArgumentParser()
    p.add_argument("--url", default=DEFAULT_URL)
    p.add_argument("--run-id")
    args = p.parse_args(argv)
    api = XvnApi(args.url)
    memory = api.get("/api/memory").payload
    proposals = []
    if args.run_id:
        exp = api.get(f"/api/eval/runs/{args.run_id}/export").payload
        fs = exp.get("filter_summaries") or []
        if fs:
            proposals.append(
                {
                    "type": "filter_pattern",
                    "text": (
                        f"Run {args.run_id} had filter wakeups={fs[0].get('wakeups')} "
                        f"over bars={fs[0].get('bars_scanned')}; review whether this is "
                        "desired gating density."
                    ),
                }
            )
        if exp.get("strategy") is None or not exp.get("agents"):
            proposals.append(
                {
                    "type": "export_integrity",
                    "text": (
                        f"Run {args.run_id} export omitted strategy or agents; investigate "
                        "export embedding before using it as canonical training/eval evidence."
                    ),
                }
            )
    print_json({"memory": memory, "proposals": proposals})
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
