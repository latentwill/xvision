#!/usr/bin/env python3
"""Remote xvn job helper for Tailscale-served xvision nodes.

Examples:
  scripts/xvn-remote.py exec eval run --strategy <id> --scenario <name> --mode backtest
  scripts/xvn-remote.py submit eval list
  scripts/xvn-remote.py status <job_id>
  scripts/xvn-remote.py output <job_id>
  scripts/xvn-remote.py cancel <job_id>
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any

DEFAULT_BASE_URL = os.environ.get("XVN_REMOTE_URL", "https://xvn.tail2bb69.ts.net")
DEFAULT_TIMEOUT_SECS = 3600
DEFAULT_POLL_INTERVAL_SECS = 1.0
TERMINAL_STATUSES = {"succeeded", "failed", "timed_out", "cancelled"}


@dataclass
class HttpResult:
    status: int
    payload: Any


class RemoteCliError(RuntimeError):
    pass


def normalize_base_url(value: str) -> str:
    return value.rstrip("/")


def request_json(method: str, url: str, body: dict[str, Any] | None = None) -> HttpResult:
    data = None if body is None else json.dumps(body).encode("utf-8")
    headers = {"Accept": "application/json"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            raw = resp.read().decode("utf-8")
            payload = json.loads(raw) if raw else None
            return HttpResult(resp.status, payload)
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RemoteCliError(f"{method} {url} -> {exc.code}: {raw or exc.reason}") from exc
    except urllib.error.URLError as exc:
        raise RemoteCliError(f"{method} {url} -> {exc.reason}") from exc


def endpoint(base_url: str, path: str) -> str:
    return f"{normalize_base_url(base_url)}{path}"


def submit_job(base_url: str, argv: list[str], timeout_secs: int) -> dict[str, Any]:
    result = request_json(
        "POST",
        endpoint(base_url, "/api/cli/jobs"),
        {"argv": argv, "timeout_secs": timeout_secs},
    )
    if not isinstance(result.payload, dict):
        raise RemoteCliError("unexpected response shape from submit")
    return result.payload


def get_job(base_url: str, job_id: str) -> dict[str, Any]:
    result = request_json("GET", endpoint(base_url, f"/api/cli/jobs/{urllib.parse.quote(job_id)}"))
    if not isinstance(result.payload, dict):
        raise RemoteCliError("unexpected response shape from status")
    return result.payload


def get_output(base_url: str, job_id: str) -> dict[str, Any]:
    result = request_json(
        "GET", endpoint(base_url, f"/api/cli/jobs/{urllib.parse.quote(job_id)}/output")
    )
    if not isinstance(result.payload, dict):
        raise RemoteCliError("unexpected response shape from output")
    return result.payload


def cancel_job(base_url: str, job_id: str) -> dict[str, Any]:
    result = request_json(
        "POST", endpoint(base_url, f"/api/cli/jobs/{urllib.parse.quote(job_id)}/cancel")
    )
    if not isinstance(result.payload, dict):
        raise RemoteCliError("unexpected response shape from cancel")
    return result.payload


def wait_for_terminal(base_url: str, job_id: str, poll_interval: float) -> dict[str, Any]:
    while True:
        meta = get_job(base_url, job_id)
        if meta.get("status") in TERMINAL_STATUSES:
            return meta
        time.sleep(poll_interval)


def print_json(payload: Any) -> None:
    json.dump(payload, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")


def cmd_submit(args: argparse.Namespace) -> int:
    payload = submit_job(args.url, args.argv, args.timeout_secs)
    print_json(payload)
    return 0


def cmd_status(args: argparse.Namespace) -> int:
    print_json(get_job(args.url, args.job_id))
    return 0


def cmd_output(args: argparse.Namespace) -> int:
    print_json(get_output(args.url, args.job_id))
    return 0


def cmd_cancel(args: argparse.Namespace) -> int:
    print_json(cancel_job(args.url, args.job_id))
    return 0


def cmd_exec(args: argparse.Namespace) -> int:
    submission = submit_job(args.url, args.argv, args.timeout_secs)
    job_id = submission["job_id"]
    meta = wait_for_terminal(args.url, job_id, args.poll_interval)
    output = get_output(args.url, job_id)

    stdout = output.get("stdout") or ""
    stderr = output.get("stderr") or ""
    if stdout:
        sys.stdout.write(stdout)
        if not stdout.endswith("\n"):
            sys.stdout.write("\n")
    if stderr:
        sys.stderr.write(stderr)
        if not stderr.endswith("\n"):
            sys.stderr.write("\n")

    status = meta.get("status")
    exit_code = output.get("exit_code")
    if isinstance(exit_code, int):
        return exit_code
    if status == "cancelled":
        return 130
    if status == "timed_out":
        return 124
    return 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Drive xvn over the remote CLI API")
    parser.add_argument(
        "--url",
        default=DEFAULT_BASE_URL,
        help=f"base URL for the remote node (default: {DEFAULT_BASE_URL})",
    )

    sub = parser.add_subparsers(dest="command", required=True)

    submit = sub.add_parser("submit", help="submit argv as a remote xvn job")
    submit.add_argument("--timeout-secs", type=int, default=DEFAULT_TIMEOUT_SECS)
    submit.add_argument("argv", nargs=argparse.REMAINDER)
    submit.set_defaults(func=cmd_submit)

    status = sub.add_parser("status", help="show job metadata")
    status.add_argument("job_id")
    status.set_defaults(func=cmd_status)

    output = sub.add_parser("output", help="show job output")
    output.add_argument("job_id")
    output.set_defaults(func=cmd_output)

    cancel = sub.add_parser("cancel", help="cancel a running job")
    cancel.add_argument("job_id")
    cancel.set_defaults(func=cmd_cancel)

    exec_cmd = sub.add_parser("exec", help="submit argv and wait for completion")
    exec_cmd.add_argument("--timeout-secs", type=int, default=DEFAULT_TIMEOUT_SECS)
    exec_cmd.add_argument("--poll-interval", type=float, default=DEFAULT_POLL_INTERVAL_SECS)
    exec_cmd.add_argument("argv", nargs=argparse.REMAINDER)
    exec_cmd.set_defaults(func=cmd_exec)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.command in {"submit", "exec"} and args.argv and args.argv[0] == "--":
        args.argv = args.argv[1:]
    if args.command in {"submit", "exec"} and not args.argv:
        parser.error(f"{args.command} requires at least one xvn argument")

    try:
        return int(args.func(args))
    except RemoteCliError as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
