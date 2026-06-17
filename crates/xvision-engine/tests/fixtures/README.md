# tests/fixtures — test-only support files

| File | Purpose |
|---|---|
| `stub_nano_worker.py` | TEST-ONLY deterministic stub for nanochat subprocess integration tests. Controlled via env vars (`STUB_DIRECTION`, `STUB_CONFIDENCE`, `STUB_SLEEP_SEC`, `STUB_EXIT_CODE`). Never deploy as a real inference worker. |
