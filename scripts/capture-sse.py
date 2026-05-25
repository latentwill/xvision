#!/usr/bin/env python3
"""Capture an xvision SSE / streamed-event endpoint to JSONL for evidence.

Phase 0 evidence helper for the chat-rail / DSPy / strategy-agents wave.
It is deliberately dependency-free (stdlib only) so it runs anywhere `xvn-remote.py`
runs, and it never prints unredacted secrets.

Two endpoint shapes are supported:

  * GET SSE streams (EventSource style), e.g. the agent-run stream:
      scripts/capture-sse.py get /api/agent-runs/<run_id>/stream \
        --out docs/superpowers/evidence/<wave>/chat-rail/unified-stream-api.jsonl \
        --expect run_started,span_started,span_finished,run_finished \
        --max-events 500 --idle-timeout 30

  * POST body streams (the chat rail), e.g. /api/chat-rail/chat:
      scripts/capture-sse.py post /api/chat-rail/chat \
        --json '{"session_id":"...","message":"hi","profile":"workspace"}' \
        --out .../chat-rail/unified-stream-cli.txt \
        --expect token,done

Exit codes:
  0  stream ended cleanly and every --expect kind was observed
  2  an expected event kind was never seen (assertion failure)
  3  HTTP / connection error
  4  usage error

The event "kind" is read from the SSE `event:` field when present, otherwise
from a `type`/`kind`/`event` field inside the JSON `data:` payload (covers both
the chat-rail `WizardEvent` shape and the agent-run `RunEvent` shape).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from typing import Any

DEFAULT_BASE_URL = os.environ.get("XVN_BASE_URL", "http://127.0.0.1:8080")

# Field names whose values are redacted anywhere they appear in a payload.
_SECRET_KEYS = re.compile(
    r"(api[_-]?key|secret|token|authorization|bearer|password|cookie|session[_-]?token)",
    re.IGNORECASE,
)
# Inline patterns (e.g. a leaked "sk-..." in free text) are masked too.
_SECRET_VALUE = re.compile(r"\b(sk-[A-Za-z0-9]{8,}|Bearer\s+[A-Za-z0-9._\-]+)\b")


def redact(value: Any) -> Any:
    """Recursively mask secret-looking keys and inline secret values."""
    if isinstance(value, dict):
        out: dict[str, Any] = {}
        for k, v in value.items():
            if isinstance(k, str) and _SECRET_KEYS.search(k):
                out[k] = "<redacted>"
            else:
                out[k] = redact(v)
        return out
    if isinstance(value, list):
        return [redact(v) for v in value]
    if isinstance(value, str):
        return _SECRET_VALUE.sub("<redacted>", value)
    return value


def event_kind(sse_event: str | None, data_obj: Any) -> str | None:
    if sse_event and sse_event not in ("message", "snapshot"):
        return sse_event
    if isinstance(data_obj, dict):
        for key in ("type", "kind", "event"):
            val = data_obj.get(key)
            if isinstance(val, str):
                return val
    return sse_event


def build_request(method: str, url: str, body: bytes | None) -> urllib.request.Request:
    headers = {"Accept": "text/event-stream"}
    if body is not None:
        headers["Content-Type"] = "application/json"
    token = os.environ.get("XVN_BEARER_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return urllib.request.Request(url, data=body, method=method.upper(), headers=headers)


def iter_sse_frames(resp, idle_timeout: float):
    """Yield (event, data_str) tuples from an SSE byte stream.

    Frames are separated by a blank line. `:` comment lines (keep-alives) are
    skipped. Multi-line data is joined with newlines per the SSE spec.
    """
    buf: list[str] = []
    event_name: str | None = None
    last_rx = time.monotonic()
    for raw in resp:
        line = raw.decode("utf-8", errors="replace").rstrip("\n")
        now = time.monotonic()
        if idle_timeout and (now - last_rx) > idle_timeout and not buf:
            break
        last_rx = now
        if line == "":
            if buf:
                yield event_name, "\n".join(buf)
            buf, event_name = [], None
            continue
        if line.startswith(":"):
            continue  # keep-alive comment
        if line.startswith("event:"):
            event_name = line[len("event:"):].strip()
        elif line.startswith("data:"):
            buf.append(line[len("data:"):].lstrip())
        # ignore id:/retry: for evidence purposes
    if buf:
        yield event_name, "\n".join(buf)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("method", choices=["get", "post"], help="HTTP method for the stream")
    ap.add_argument("path", help="endpoint path (joined to --base-url) or full URL")
    ap.add_argument("--base-url", default=DEFAULT_BASE_URL, help=f"default {DEFAULT_BASE_URL} (env XVN_BASE_URL)")
    ap.add_argument("--json", dest="json_body", help="JSON request body for POST")
    ap.add_argument("--out", help="JSONL output file (also printed to stdout if omitted)")
    ap.add_argument("--expect", help="comma-separated event kinds that MUST appear")
    ap.add_argument("--max-events", type=int, default=0, help="stop after N events (0 = unbounded)")
    ap.add_argument("--idle-timeout", type=float, default=30.0, help="seconds of silence before stopping")
    args = ap.parse_args()

    url = args.path if args.path.startswith("http") else args.base_url.rstrip("/") + args.path
    body = None
    if args.method == "post":
        body = (args.json_body or "{}").encode("utf-8")

    expected = {k.strip() for k in args.expect.split(",")} if args.expect else set()
    seen_kinds: set[str] = set()
    count = 0

    out_fh = open(args.out, "w", encoding="utf-8") if args.out else None
    try:
        req = build_request(args.method, url, body)
        with urllib.request.urlopen(req, timeout=args.idle_timeout or None) as resp:
            for sse_event, data_str in iter_sse_frames(resp, args.idle_timeout):
                try:
                    data_obj = json.loads(data_str)
                except json.JSONDecodeError:
                    data_obj = data_str
                kind = event_kind(sse_event, data_obj)
                if kind:
                    seen_kinds.add(kind)
                record = {
                    "seq": count,
                    "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                    "event": sse_event,
                    "kind": kind,
                    "data": redact(data_obj),
                }
                line = json.dumps(record, ensure_ascii=False)
                if out_fh:
                    out_fh.write(line + "\n")
                    out_fh.flush()
                else:
                    print(line)
                count += 1
                if args.max_events and count >= args.max_events:
                    break
    except urllib.error.HTTPError as exc:
        sys.stderr.write(f"HTTP {exc.code} {exc.reason} for {args.method.upper()} {url}\n")
        return 3
    except urllib.error.URLError as exc:
        sys.stderr.write(f"connection error for {args.method.upper()} {url}: {exc.reason}\n")
        return 3
    finally:
        if out_fh:
            out_fh.close()

    sys.stderr.write(f"captured {count} events; kinds seen: {sorted(seen_kinds)}\n")
    missing = expected - seen_kinds
    if missing:
        sys.stderr.write(f"ASSERTION FAILED: expected kinds never seen: {sorted(missing)}\n")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
