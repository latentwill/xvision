#!/usr/bin/env python3
# =============================================================================
# TEST-ONLY — stub_nano_worker.py
# =============================================================================
# Deterministic stub for nanochat inference integration tests.
# NEVER use this as a production inference worker.
# Driven by env vars; see crates/xvision-engine/tests/nano_inference_subprocess.rs.
# Real inference worker: nanochat/infer.py (built in Phase 5).
# =============================================================================
# Test-only stub. Reads one JSON line from stdin, writes one JSON line to stdout.
# Controlled by env vars:
#   STUB_DIRECTION  — LONG | SHORT | NEUTRAL (default LONG)
#   STUB_CONFIDENCE — float 0-1 (default 0.9)
#   STUB_SLEEP_SEC  — sleep this many seconds before responding (timeout test)
#   STUB_EXIT_CODE  — exit with this code instead of 0 (crash test)
import sys, json, os, time

sleep_sec = float(os.environ.get("STUB_SLEEP_SEC", "0"))
exit_code  = int(os.environ.get("STUB_EXIT_CODE", "0"))
direction  = os.environ.get("STUB_DIRECTION", "LONG")
confidence = float(os.environ.get("STUB_CONFIDENCE", "0.9"))

# Consume stdin (required by the protocol even if ignored).
_ = sys.stdin.readline()

if sleep_sec > 0:
    time.sleep(sleep_sec)

if exit_code != 0:
    sys.exit(exit_code)

print(json.dumps({"direction": direction, "confidence": confidence}), flush=True)
