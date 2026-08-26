#!/usr/bin/env python3
"""Reverse proxy for opencode-zen free tier.

Two problems this solves:
1. Free tier rejects `response_format` (json_object/json_schema) — stripped
   before forwarding. The xvision engine embeds the same JSON Schema contract
   in the prompt, so output validation still happens engine-side.
2. Free tier intermittently returns 503/"Internal server error" (~50% of
   calls). Retries with backoff until a 2xx arrives (max 6 attempts).

Uses curl as transport (python TLS fingerprints get Cloudflare-challenged).
Streams the body through so SSE works.

Usage: python3 zen_strip_proxy.py [listen_port]   (default 8787)
"""
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

UPSTREAM = "https://opencode.ai/zen"
MAX_ATTEMPTS = 6
HOP_HEADERS = {
    "connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
    "te", "trailers", "transfer-encoding", "upgrade", "host",
    "content-length", "accept-encoding",
}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        sys.stderr.write("%s %s\n" % (self.command, self.path))

    def _forward(self, method):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""

        # Strip the decoy ":11434" path segment (used to steer xvision's
        # provider map onto the ollama carrier, which speaks chat
        # completions instead of the Responses API).
        i = self.path.find("/v1/")
        path = self.path[i:] if i > 0 else self.path

        if raw and path.endswith("/chat/completions"):
            try:
                payload = json.loads(raw)
                if isinstance(payload, dict) and "response_format" in payload:
                    payload.pop("response_format")
                    raw = json.dumps(payload).encode()
                    sys.stderr.write("stripped response_format\n")
            except Exception:
                pass

        auth = self.headers.get("Authorization", "")
        ct = self.headers.get("Content-Type", "application/json")

        tmp_body = tempfile.NamedTemporaryFile(delete=False, suffix=".json")
        tmp_body.write(raw if raw else b"")
        tmp_body.close()
        tmp_hdrs = tempfile.NamedTemporaryFile(delete=False, suffix=".hdrs")
        tmp_hdrs.close()

        cmd = [
            "curl", "-sS", "-N", "--no-buffer", "--http1.1",
            "--max-time", "900",
            "-X", method,
            "-H", f"Authorization: {auth}",
            "-H", f"Content-Type: {ct}",
            "-H", "User-Agent: curl/8.7.1",
            "-H", "Accept: */*",
            "--data-binary", f"@{tmp_body.name}",
            "-D", tmp_hdrs.name,
        ]
        if method == "GET":
            cmd = [c for c in cmd if c != "--data-binary" or True]
            # curl errors on -X GET with data; rebuild cleanly instead
            cmd = ["curl", "-sS", "-N", "--no-buffer", "--http1.1",
                   "--max-time", "900",
                   "-H", f"Authorization: {auth}",
                   "-H", "Accept: */*",
                   "-D", tmp_hdrs.name,
                   UPSTREAM + path]

        def one_attempt():
            if method != "GET":
                cmd_full = cmd[:cmd.index("-D")] + ["-D"] if False else None
            return None

        status_sent = False
        try:
            for attempt in range(1, MAX_ATTEMPTS + 1):
                if method != "GET":
                    full = cmd + [UPSTREAM + path]
                    full[full.index("-D") + 1] = tmp_hdrs.name
                    full[full.index("--data-binary") + 1] = f"@{tmp_body.name}"
                else:
                    full = cmd[:]
                    full[full.index("-D") + 1] = tmp_hdrs.name

                if os.path.exists(tmp_hdrs.name):
                    os.unlink(tmp_hdrs.name)

                proc = subprocess.Popen(full, stdout=subprocess.PIPE)

                # Wait for response headers.
                status = None
                for _ in range(1200):
                    time.sleep(0.25)
                    if os.path.exists(tmp_hdrs.name) and os.path.getsize(tmp_hdrs.name) > 0:
                        with open(tmp_hdrs.name, "rb") as hf:
                            first = hf.readline().decode(errors="replace")
                        if first.startswith("HTTP"):
                            status = int(first.split()[1])
                            break
                    if proc.poll() is not None:
                        break

                if status is None:
                    proc.kill()
                    proc.wait()
                    sys.stderr.write(f"attempt {attempt}: no status\n")
                    time.sleep(min(2 ** attempt, 20))
                    continue

                if status >= 500 and attempt < MAX_ATTEMPTS:
                    proc.kill()
                    proc.wait()
                    sys.stderr.write(f"attempt {attempt}: HTTP {status}, retrying\n")
                    time.sleep(min(2 ** attempt, 20))
                    continue

                body = proc.stdout.read()
                proc.wait()

                # The zen relay reports upstream failures as HTTP 200 SSE
                # with finish_reason "network_error" — retry those too.
                if (status >= 500 or b'"finish_reason":"network_error"' in body
                        or b'"finish_reason": "network_error"' in body):
                    sys.stderr.write(f"attempt {attempt}: HTTP {status} network_error={b'network_error' in body}\n")
                    if attempt < MAX_ATTEMPTS:
                        time.sleep(min(2 ** attempt, 20))
                        continue

                if not status_sent:
                    self.send_response(status)
                    if path.endswith("/chat/completions"):
                        self.send_header("Content-Type", "text/event-stream")
                        self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    status_sent = True
                self.wfile.write(body)
                self.wfile.flush()
                break
            if not status_sent:
                self.send_response(502)
                self.send_header("Content-Length", "0")
                self.end_headers()
        except BrokenPipeError:
            pass
        finally:
            for p in (tmp_body.name, tmp_hdrs.name):
                try:
                    os.unlink(p)
                except OSError:
                    pass

    def do_POST(self):
        self._forward("POST")

    def do_GET(self):
        self._forward("GET")


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8787
    ThreadingHTTPServer(("0.0.0.0", port), Handler).serve_forever()
