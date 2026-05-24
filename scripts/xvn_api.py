#!/usr/bin/env python3
"""Small direct HTTP client for the xvision dashboard API.

Use this for dashboard API routes. Prefer ``scripts/xvn-remote.py`` for
allowlisted CLI jobs; use this client when the dashboard API is the intended
surface, especially for read-only JSON diagnostics and explicit authoring
workflows that the remote CLI intentionally blocks.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any

DEFAULT_BASE_URL = os.environ.get("XVN_BASE_URL", "https://xvn.tail2bb69.ts.net")
DEFAULT_TIMEOUT_SECS = 60


@dataclass
class ApiResponse:
    method: str
    url: str
    status: int
    payload: Any
    raw: str


class XvnApiError(RuntimeError):
    pass


class XvnApi:
    def __init__(self, base_url: str | None = None, timeout: int = DEFAULT_TIMEOUT_SECS):
        self.base_url = (base_url or DEFAULT_BASE_URL).rstrip("/")
        self.timeout = timeout

    def url(self, path: str) -> str:
        return self.base_url + (path if path.startswith("/") else "/" + path)

    def request(self, method: str, path: str, body: Any | None = None) -> ApiResponse:
        data = None if body is None else json.dumps(body).encode("utf-8")
        headers = {"Accept": "application/json"}
        if data is not None:
            headers["Content-Type"] = "application/json"
        url = self.url(path)
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                raw = resp.read().decode("utf-8", "replace")
                payload = json.loads(raw) if raw else None
                return ApiResponse(method, url, resp.status, payload, raw)
        except urllib.error.HTTPError as exc:
            raw = exc.read().decode("utf-8", "replace")
            raise XvnApiError(f"{method} {url} -> {exc.code}: {raw or exc.reason}") from exc
        except urllib.error.URLError as exc:
            raise XvnApiError(f"{method} {url} -> {exc.reason}") from exc

    def get(self, path: str) -> ApiResponse:
        return self.request("GET", path)

    def post(self, path: str, body: Any) -> ApiResponse:
        return self.request("POST", path, body)

    def patch(self, path: str, body: Any) -> ApiResponse:
        return self.request("PATCH", path, body)

    def put(self, path: str, body: Any) -> ApiResponse:
        return self.request("PUT", path, body)

    def delete(self, path: str) -> ApiResponse:
        return self.request("DELETE", path)


def print_json(obj: Any) -> None:
    json.dump(obj, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Call a xvision dashboard API route")
    p.add_argument("--url", default=DEFAULT_BASE_URL)
    p.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT_SECS)
    p.add_argument("method", choices=["GET", "POST", "PATCH", "PUT", "DELETE"])
    p.add_argument("path")
    p.add_argument("--body", help="JSON body string or @file.json")
    args = p.parse_args(argv)

    body = None
    if args.body:
        if args.body.startswith("@"):
            with open(args.body[1:], encoding="utf-8") as body_file:
                text = body_file.read()
        else:
            text = args.body
        body = json.loads(text)

    api = XvnApi(args.url, timeout=args.timeout)
    try:
        resp = api.request(args.method, args.path, body)
        print_json({"status": resp.status, "payload": resp.payload})
        return 0
    except XvnApiError as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
