# Autooptimizer Ollama Run — Findings (2026-06-08)

Attempted a 5-iteration optimizer run using ollama models for experiment writer
(mutator), reviewer (judge), and the strategy's trader.  Ran OOM before any
cycle completed; root cause traced to the model allowlist inconsistency below.

---

## F1 (HIGH) — `build_dispatch` bypasses `enabled_models`; paper-test enforces it

**What happened:** `xvn provider models --name ollama` showed all five catalog
models — including `gemma4:26b-mlx` which is NOT in `enabled_models`.  When
that model was used for the cycle, the mutator/judge accepted it (their dispatch
path never checks the allowlist), but the paper-test backtest failed mid-cycle
with `model_disabled` because `resolve_provider()` in the engine enforces the
allowlist.

**Root cause:**

| Path | File | Allowlist check? |
|---|---|---|
| Chat-rail dispatch | `crates/xvision-dashboard/src/llm_dispatch.rs:95` | ✅ yes |
| Eval / strategy execution | `crates/xvision-engine/src/api/settings/providers.rs:448` | ✅ yes (`resolve_provider`) |
| Autooptimizer CLI `build_dispatch` | `crates/xvision-cli/src/commands/autooptimizer.rs:1921` | ❌ no |
| Autooptimizer dashboard `build_autooptimizer_dispatch` | `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:509` | ❌ no |

**Fix (this PR):** Added `enabled_models` guard to both paths.  CLI now returns
a clear `CliError::usage` at startup; dashboard returns a `DashboardError::Validation`
before the cycle is enqueued.  Error message names the blocked model and tells
the operator how to add it with `xvn provider models --name <p> --enable <m>`.

---

## F2 (HIGH) — OOM when loading `gemma4:26b-mlx` under memory pressure

**What happened:** The live node (extndly-dev, ~3.7 GiB RAM) OOM-killed the
`xvn-app` container partway through strategy execution when `gemma4:26b-mlx`
(26 B parameter model) was loaded by ollama.

**Root cause:** `gemma4:26b-mlx` requires ~16 GB of GPU/RAM depending on
quantization. The node is RAM-constrained; even a Q4 GGUF variant is likely to
exceed available memory.

**Recommendation:** Do not use `gemma4:26b-mlx` for optimizer paper-test
runs on extndly-dev.  Use `lfm2.5:8b` or `hf.co/unsloth/Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL`
instead — both are in `enabled_models` and fit in available RAM.

**Note:** `gemma4:26b-mlx` should NOT be added to `enabled_models` for this
node until RAM is expanded.

---

## F3 (LOW) — No cancel API for mid-run optimizer cycles

**What happened:** When the container OOM-killed, 3 cycle nodes from a previous
session remained with `status = "active"` in the DB (cycle IDs:
`01KTARKMPVNQPVFXF9B5R7AQTZ`, `01KTARHXF3E3Y933S00ABKKX49`,
`01KT9KSRMCCZ3P8DR7VZZBXAAW`).  These are zombie rows — the processes are dead
— but `GET /api/autooptimizer/cycles` still reports `active_count = 1` for each.

**Root cause:** There is no `DELETE /api/autooptimizer/cycles/:id` or cancel
endpoint.  The prior workaround was `docker restart xvn-app`, which kills the
process but does not update DB state.

**Recommendation:** Add a `POST /api/autooptimizer/cycles/:id/cancel` endpoint
that sets `status = 'cancelled'` on the node row.  As a short-term cleanup, a
migration or admin command to mark orphaned-active nodes as cancelled after a
container restart would prevent the stale active counts from polluting the UI.

---

## F4 (LOW) — `PUT /api/settings/providers/:name` returns 405; only `/enabled-models` sub-route exists

**What happened:** Attempted `PATCH /api/settings/providers/ollama` with
`{ "enabled_models": [...] }` — got 405.

**Root cause:** The correct endpoint is
`PUT /api/settings/providers/:name/enabled-models` (see `server.rs:57`).
There is no PATCH on the parent route.

**Recommendation:** This is fine as-is once PR #859 (`xvn provider models
--enable/--disable`) lands and is deployed.  Document the correct endpoint
in the operator runbook.

---

## Recommended model config for ollama optimizer runs on extndly-dev

```
mutator/judge:  ollama / lfm2.5:8b
strategy trader: ollama / lfm2.5:8b  (or Qwen3-4B-Instruct if better quality needed)
```

Both models are already in `enabled_models`.  Keep `gemma4:26b-mlx` off the
allowlist until the node has more RAM.
