# AutoOptimizer Run-6 — live-cycle verification on `deploy-latest` (post F35.3/F29/F36)

**Date:** 2026-06-05
**Deploy under test:** `xvision:deploy-latest`, image built `2026-06-05T01:47:24Z` — includes PR **#821** (F35.3 + F29), merged `01:32Z`. Verified the running CLI exposes the F28/F24 flags and the F35.3 route.
**Live node:** `xvn-app` (deploy-latest) reached via `https://xvn.tail2bb69.ts.net` (the container publishes no host port; see ops note).
**Test:** two bounded CLI cycles, same parent + model + windows, to exercise mutator diversity (F32), duplicate-candidate attribution (F33), and the cost surfaces (F11/F23/F35/F35.3).

```
xvn optimizer run-cycle --strategy 01KT20AS9674W1THXPWR93C1GX \
  --provider openrouter --model google/gemini-3.1-flash-lite --budget 1.00 \
  --day-start 2025-04-01 --day-end 2025-04-04 \
  --baseline-start 2025-04-05 --baseline-end 2025-04-08 --objective sharpe
```

Cycle A = `01KTARHXF3E3Y933S00ABKKX49`  ·  Cycle B = `01KTARKMPVNQPVFXF9B5R7AQTZ`  ·  parent `e3f9f8f378…`

---

## Verified working ✅
- **Cost metering (F11/F23)** — both cycles metered real spend: A `$0.0211` (78 821/899 tok), B `$0.0210` (78 837/893 tok). 0 unpriced calls. Budget cap (`--budget 1.00`) accepted and respected.
- **F35 #2 (detail/list cost fields)** — `GET /api/autooptimizer/cycles` and `GET /api/autooptimizer/cycles/:id` both return `cost_usd / input_tokens / output_tokens / unpriced_calls` for completed cycles.
- **F35.3 (live cost endpoint)** — `GET /api/autooptimizer/cycles/:id/cost` returns `{cost_usd, input_tokens, output_tokens, unpriced_calls, recorded:true}`. (Could not observe *mid-cycle* streaming — bounded cycles complete in ~15 s, faster than the ~10 s ticker is meant for; endpoint itself is correct.)
- **F29 retire** — `POST /api/autooptimizer/lineage/:hash/retire` route present (GET → 405, POST-only).
- **Honesty check** — sabotage variant `kill-trades` correctly rejected by the gate in both cycles.
- **F24 objective / F28 window+budget flags** — all present on `run-cycle --help` and functional.

---

## 🔴 BLOCKER — F32 mutator diversity is NOT effective on the real LLM path (the fix is stub-validated only)
Two cycles with **distinct** `cycle_id`s — therefore distinct exploration seeds, distinct sampling temperatures, and distinct prompt nonces — off the same parent `e3f9f8f378` both produced the **byte-identical** candidate:

```
child_hash = b5505dd671336c3f03ffa8d157db71db3c45dddfc802d07a14d661f84f945a74   (both A and B)
```

This is the **same fixed-point candidate** the Run-5 findings reported *before* F32. The optimizer still cannot explore, so it cannot converge: every repeat cycle re-proposes and re-backtests the identical losing candidate (pure token/$ waste; ties F20 — a KEPT live improvement stays unreachable).

**Root cause.** The F32 mechanism *is* wired (`mutator.rs:281` sets `temperature: Some(exploration_temperature(seed))` in the 0.7–1.1 band; `mutator.rs:409` injects an "Exploration directive (variant N)" nonce; `cycle.rs:271` `exploration_seed_for(cycle_id, idx)` is unique per cycle). I confirmed `temperature` is forwarded into the real openrouter body (`agent/llm.rs::openai_compat_request_body`). **But** the only thing proving "diversity" is the merged test `crates/xvision-engine/tests/autooptimizer_mutator_diversity.rs`, which uses a **`SeedSensitiveDispatch` stub** that mechanically maps `seed → ema_fast value`. It asserts "the seed reaches the prompt," **not** "the real model produces diverse output." On the real `gemini-3.1-flash-lite` path, the constrained structured-JSON experiment (a single obvious param edit) collapses to the same proposal even at temp 1.1 + nonce → no diversity.

**Acceptance (sharpened).** The diversity guarantee must be exercised against a *non-stub* dispatch, or enforced structurally rather than hoping the LLM samples differently. Options to evaluate: (a) raise `mutations_per_parent` with an explicit dedup-and-resample-until-distinct loop; (b) widen the temperature band and/or inject the prior cycle's candidate as a "propose something *different from this*" negative example; (c) deterministically perturb the proposed param post-LLM when a duplicate hash is detected. A live (or recorded-LLM) test must assert ≥2 distinct hashes from N cycles on one parent. The current stub test gives false confidence.

---

## 🟠 F33 incomplete — duplicate-candidate per-cycle attribution still surfaces first-writer globals
Because F32 makes every repeat cycle re-derive `b5505dd671` (the **common** case, not an edge case), the duplicate path matters. The per-cycle views disagree on what each cycle did:

| Surface | Reports for cycle A/B |
|---|---|
| `run-cycle` CLI summary | **`mutation_gated passed:false`** → "candidates: 1 gated (0 kept, 1 **dropped**)" |
| `xvn optimizer inspect <A or B>` | "candidates: 1 (1 **kept**, 0 dropped)", node Status **kept** |
| `GET /cycles/:id` (A) | `active_count:1, rejected_count:0`; embedded node `gate_verdict:"Pass", status:"active", cycle_id:"01KT9KSRMC…"` ← the **original** cycle, not A |
| `xvn optimizer lineage ls --cycle <A or B>` | **"(no experiments)"** — yet `inspect <A>` shows the node |
| global `lineage ls` | A/B invisible; row still shows original cycle `01KT9KSRMC` + Gate "passed" |

So a cycle that **dropped** its candidate is reported by `inspect` and the dashboard API as having **kept an active candidate**, attributed to a different cycle, with a "Pass" verdict and the original `created_at`. F33 added a per-cycle edge that `inspect` reads, but `gate_verdict`/`status`/`cycle_id`/`created_at`/counts surfaced for a duplicate are still the content node's first-writer globals, and `lineage ls --cycle` was never taught about the per-cycle edge.

**Acceptance.** For a duplicate candidate, each cycle's detail/inspect must reflect **that cycle's** gate verdict and rejection count (record verdict on the per-cycle edge, not only the node); `lineage ls --cycle <id>` must return the same nodes `inspect <id>` does. Add a test: two cycles re-derive one hash where cycle 1 keeps and cycle 2 drops → each cycle's detail shows its own verdict/counts.

---

## 🟡 Ops note (not code) — stale containers own the host dashboard ports
`xvn-app` / `xvnej-app` (current `deploy-latest`) run with `network_mode: container:ts-xvn(ej)` and publish **no host ports** — they're only reachable via the Tailscale nodes. Host `:8788`/`:8789` are still owned by two **unhealthy, 20-h-old** `ghcr.io/latentwill/xvision:sha-2e012a1` containers (Coolify-managed leftovers). Anyone who `curl`s `localhost:8788` hits the **stale** build (returns `no route for /api/autooptimizer/cycles`) and will wrongly conclude the new routes are missing. Recommend pruning the stale `sha-2e012a1` containers.

---

## Suggested next pass
1. **F32 (blocker)** — make diversity real (structural dedup-resample) and test it against a non-stub dispatch. Nothing else in the optimizer matters until successive cycles actually differ.
2. **F33 (incomplete)** — per-cycle verdict/counts for duplicate candidates; fix `lineage ls --cycle`.
3. **Ops** — prune stale `sha-2e012a1` containers holding host `:8788/:8789`.

## Status recap
- **Verified on deploy-latest:** F11, F23, F24, F28 flags, F29, F34 (lock present), F35 #1/#2, **F35.3**, honesty gate.
- **Still open:** **F32** (diversity ineffective on real LLM — was thought fixed), **F33** (duplicate attribution incomplete), F25 (deferred), plus the ops cleanup above.
