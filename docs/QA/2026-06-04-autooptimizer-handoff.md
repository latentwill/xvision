# AutoOptimizer — Handoff / State of the System (2026-06-04)

Orientation doc for whoever picks up the autooptimizer next. Summarizes the
2026-06-03/04 QA + fix campaign (findings F1–F22 across four run passes), the
current working state, what remains, and how to run/verify it. Per-pass detail
lives in the dated `docs/QA/2026-06-04-autooptimizer-*.md` docs.

> **Naming:** developer-surface = `autooptimizer` / `AutoOptimizer` (Rust module,
> `/api/autooptimizer/*`, `autooptimizer_*` tables); operator-surface = **Optimizer**
> / `xvn optimizer`. Distinct from the unrelated DSPy `xvn optimize` /
> `optimization_*` surface — do not conflate.

## Where it stands now (image 13:09Z, PR #805/#806)

**The optimizer works end-to-end.** A real `xvn optimizer run-cycle` on a real
strategy now: selects the parent → the experiment writer (mutator) emits a
**distinct, valid `risk.*` param candidate** → backtests it on a day + a
held-out baseline window through the **same eval engine** as `xvn eval run` →
gates on Δsharpe vs `min_improvement` → keeps or drops → runs the honesty-check
canary → records lineage → surfaces in CLI + dashboard. Verified live (cycle
`01KT9CB0BAHQ2Q7HQFEJKFGDQG`: candidate `b5505dd671`, Day Sharpe -0.985 → correctly
dropped). The keep path is covered by `run_cycle_keeps_improving_risk_param_candidate`.

### Fixed + verified (F1–F21 except the two below)
- **Launch/exec:** real cycles run (F1 paper-test slot resolution), provider/model
  overrides for mutator+judge (T2), `$XVN_HOME` paths (T1), provider-key secrets
  fallback (T3), window-override flags `--day-start/--day-end/--baseline-*` (F3).
- **Candidate generation:** mutator emits distinct valid candidates targeting
  `risk.*` (the unlock — real strategies have empty `mechanical_params`); identity
  diffs rejected; no-candidate iterations emit a typed `no_candidate` event (F14,
  F15, F20, F21).
- **Lineage integrity:** re-runs unblocked (rejected node reseeds as active root),
  cycle-safe ancestry, no self-parent nodes (F12).
- **Visibility:** unified lineage store in `xvn.db`; `optimizer ls` + `inspect`
  show mutation cycles with Day/Hold Sharpe; `GET /api/autooptimizer/cycles[/:id]`;
  genealogy `/api/autooptimizer/lineage`; blob/diff `/api/autooptimizer/blob/:hash`
  (F8, F13, F19).
- **Canary:** labeled honesty-check outcome (`sabotage_variant` + message); raw
  `min_order_size` WARN flood removed (F9).
- **Harness parity:** optimizer shares the eval engine + a single
  `synthesize_optimizer_day_scenario`; parity test
  `optimizer_adapter_matches_direct_eval_executor` (F10).
- **Ergonomics:** `mutate-once` blob-dir default (F16), `flywheel` global view
  (F17), `demo` resilient to unknown event variants (F18).

### Open (2)
- **F11 — cost reporting / `--budget` is blind.** `cycle cost:` prints `$0.00`
  while `model_calls.cost_usd` holds the real spend (~$0.13/cycle on the small
  windows; ~$2 on the full default window). The meter doesn't read realized cost.
  **Fix:** sum `model_calls.cost_usd` over the cycle's run_ids. Until then the
  budget ceiling never trips — treat `--budget` as non-functional and bound cost
  by window size + `mutations_per_parent` instead.
- **F22 — agentless/mechanical strategies crash.** A strategy with no declared
  agent (e.g. `example-trend-follower`) resolves a default `anthropic.claude-sonnet-4.6`
  trader and routes it to the cycle's openrouter dispatch → 400. The F22 preflight
  only covers strategies that *declare* an agent on a mismatched provider, not the
  agentless-default-model path. **Fix:** make agentless strategies run rules-only
  (no LLM trader) or fail fast with "provider anthropic not registered" guidance;
  optionally a paper-test model override.

### Not-yet-observed (watch, not a bug)
- No live cycle has **kept** an improvement yet — each cycle tries one risk tweak
  and usually doesn't beat `min_improvement = 0.05`. Consider more
  `mutations_per_parent` / a smarter mutator objective so it converges. Gated on
  F11 (can't safely widen the search without a working budget).

## How to run / verify

```bash
# Cheap, bounded real cycle (cached BTC windows, cheap model). Use a strategy whose
# agent provider is registered here (openrouter/deepseek) — the gemini_* family.
docker exec xvn-app xvn optimizer run-cycle \
  --strategy <id> --provider openrouter --model google/gemini-3.1-flash-lite \
  --day-start 2024-01-01 --day-end 2024-02-01 \
  --baseline-start 2025-01-01 --baseline-end 2025-02-01 --budget 3

# Verify (CLI)
docker exec xvn-app xvn optimizer ls                 # Mutation cycles section
docker exec xvn-app xvn optimizer inspect <cycle_id> # candidates, Day/Hold Sharpe, gate, honesty check
docker exec xvn-app xvn optimizer lineage ls         # full node graph

# Verify (dashboard API; token in $XVN_DASHBOARD_TOKEN)
curl -H "Authorization: Bearer $TOK" $BASE/api/autooptimizer/cycles
curl -H "Authorization: Bearer $TOK" $BASE/api/autooptimizer/cycles/<cycle_id>
curl -H "Authorization: Bearer $TOK" $BASE/api/autooptimizer/lineage
```

Real cost (until F11): `select model, count(*), sum(input_token_count), total(cost_usd)
from model_calls …` against `$XVN_HOME/xvn.db`.

## Gotchas / environment
- **Strategy mutatability:** real strategies (`gemini_*`) tune via `risk.*` (works
  now); only the 3 seeded `example_*` strategies have `mechanical_params`, and they
  hit F22. Pick a `gemini_*` strategy for end-to-end tests.
- **Provider availability on this node:** only `openrouter` + `deepseek` are
  registered (no `anthropic`). `--provider/--model` override the mutator/judge;
  the **paper-test trader uses the strategy's own agent model** (intended, for
  eval interchangeability) — so the strategy's agent provider must be registered.
- **Provider catalog is stale** (`xvn provider models` shows openrouter prices as
  `—`); `model_calls` still prices via provider-reported cost, so the ledger is the
  source of truth for spend. A `provider refresh-models` may repopulate catalog
  pricing (operational, not code).
- **Build/deploy:** local `cargo` OOMs the deploy box — fixes ship via CI/GHCR;
  verify against the running container's image, not a local build.

## Suggested next steps
1. **F11** — meter from `model_calls.cost_usd`; re-enable a real `--budget` ceiling.
2. **F22** — agentless strategies: rules-only path or fail-fast preflight + optional paper-test model override.
3. **Convergence** — once budget works, raise `mutations_per_parent` / improve the
   mutator objective and confirm a live KEPT improvement on a `gemini_*` strategy.

## Doc index (this campaign)
- `2026-06-04-autooptimizer-run-cycle-exploration-findings.md` — F1–F7
- `2026-06-04-autooptimizer-run-verification-and-harness-parity.md` — F8–F10
- `2026-06-04-autooptimizer-surface-audit-run2-findings.md` — F11–F19
- `2026-06-04-autooptimizer-run3-verification-and-deep-findings.md` — F20–F22
- `2026-06-04-autooptimizer-run4-findings.md` — verification: loop works; F11/F22 remain
- `2026-06-04-autooptimizer-handoff.md` — this doc
