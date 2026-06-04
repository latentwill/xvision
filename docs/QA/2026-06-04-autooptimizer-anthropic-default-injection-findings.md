# Hardcoded `anthropic` default-injection + image verification (F30, F22 correction)

**Date:** 2026-06-04
**Deploy under test:** `xvision:deploy-latest` (image built 13:57Z = 21:57 +0800) = PRs **#807 (F22) + #808 (F11)**. **#809 (F23) is NOT in this image** (merged 22:07 +0800, ~10 min after the build; confirmed: no `cycle_cost` table in the running DB).
**Findings only — no code.**

---

## F30 — [HIGH] `anthropic.claude-*` is hardcoded as a default where the operator chose openrouter

A strategy "designed with openrouter" ends up bound to anthropic because production code **injects an anthropic default**, even on a node where anthropic is not a registered provider (this node has only `openrouter` + `deepseek`). This is the root cause of the F22 example failure, and per the operator it's a **recurring** bug ("was in the code early on too").

### Confirmed production default-injection
- **Example seeder** `crates/xvision-cli/src/commands/example/seed.rs` (production, *before* the `#[cfg(test)]` at L422):
  - L26–27: seeded example trader → `provider: "anthropic"`, `model: "claude-haiku-4-5"`
  - L59: `attested_with: vec!["anthropic.claude-haiku-4-5"]`
  - (and the other seeded examples similarly)
  - **Inconsistency in the same file:** L49–50 correctly point the *autooptimizer config* at `provider = "openrouter"`, `model = "google/gemini-3.1-flash-lite"`. So the file already knows the right registered provider — but seeds the example *agents* to anthropic anyway.
- **On-disk examples** (`$XVN_HOME/strategies/example-*.json`): bound to `anthropic.claude-sonnet-4.6` (an older default than the seeder's current haiku — i.e. the hardcoded anthropic default has *drifted over time but always stayed anthropic*). These are unusable on an openrouter-only node and are exactly what tripped F22.

### Audit candidates (need production-vs-test triage before changing)
These match `provider:"anthropic"` / `model:"claude-sonnet-4-6"` outside an obvious catalog/pricing context, but I did **not** confirm each is production (several nearby siblings turned out to be `#[cfg(test)]` fixtures — e.g. `authoring.rs:931`, `api/strategy.rs:2029`, `autooptimizer.rs:1931/2058` are tests). The coding agent should verify each is a live default-injection, not a fixture:
- `crates/xvision-engine/src/agents/store.rs:669–670`
- `crates/xvision-engine/src/agents/validator.rs:314–315`
- `crates/xvision-engine/src/agent/execute_cline.rs:847–848`
- `crates/xvision-engine/src/agent/recovery.rs:1169`
- `crates/xvision-engine/src/eval/review/{payload,prompt,parser}.rs` (review-model defaults — may be intentional, confirm)

### Explicit non-goals (do NOT touch — legitimate)
anthropic is a first-class supported provider; these are correct and must stay:
- The anthropic SDK/client path (`agent/llm.rs`), provider **catalog** (`core/providers/catalog.rs`), **pricing**/`model_metadata`, `providers/fetcher.rs`, redaction (`observability/redactor.rs`).
- Test fixtures that intentionally exercise the anthropic path (`#[cfg(test)]`).
- Knowledge-cutoff / docs strings.

### Acceptance
1. Seeded examples (and any "default model" fallback) bind to a **registered/configured** provider — derive from the workspace's registered providers (prefer the same `openrouter/google/gemini-3.1-flash-lite` the seeder already uses for the autooptimizer config), or require an explicit `provider/model`. No literal `anthropic.claude-*` defaults in production seeding/authoring.
2. On a node without anthropic, `xvn example seed` produces strategies that are immediately runnable (eval + optimizer) with no provider mismatch.
3. A test asserts seeded examples resolve to a registered provider (so the regression can't silently return).

---

## F22 — correction: "partial," not fixed
The #807 preflight is a real improvement (fast-fail instead of a mid-run 400), but:
1. **The guidance is misleading.** It leads with *"Re-run with `--provider anthropic`"* — a provider the operator doesn't have and never configured. It should lead with "this strategy's trader is pinned to the unregistered provider 'anthropic'; re-author it onto a registered provider (openrouter/deepseek)" — i.e. point at F30, not at standing up anthropic.
2. **Root cause is F30**, not operator input: the operator *did* pass `--provider openrouter`; the anthropic binding comes from the seeded strategy, and `--provider/--model` only override the mutator/judge (the paper-test trader intentionally uses the strategy's own model — correct for interchangeability). So the real fix is F30 (don't seed anthropic), plus better preflight wording.

Marking F22 **partial** (was prematurely called ✅).

---

## Image verification (this deploy: #807 + #808)
| # | Result |
|---|--------|
| **F11** (realized-cost metering) | **✅ FIXED** — a real v3 cycle printed `cycle cost: $0.88 (metered paper-test + experiment-writer/judge inference)` (was `$0.00`). Metering at the dispatch boundary works. |
| **F22** (agentless preflight) | **⚠️ PARTIAL** — fast-fails with a clear mismatch message (no more mid-run 400), but guidance is misleading + root cause F30 remains. |
| **F23** (token surfacing) | **⏳ NOT IN THIS IMAGE** — #809 merged 10 min after the build; no `cycle_cost` table, no `tokens:` line in CLI/`inspect`/API. Verify on the next deploy. |
| Core loop | **✅** — real candidate `b5505dd671` proposed, gated on real Day/Hold Sharpe, dropped (no improvement); honesty check labeled. |

## Artifacts
- v3 run: `/root/xvn-work/night-watch/optrun-v3-f11f23-141457.log` (cycle `01KT9FSFREBMAM4JWNKPF2XTWY`, cost $0.88)
- F22 fast-fail message: `xvn optimizer run-cycle --strategy example-trend-follower --provider openrouter …`
- Seeder inconsistency: `example/seed.rs` L26-27/L59 (anthropic) vs L49-50 (openrouter)
- Image timing: build 21:57 +0800; #808 merged 21:49 (in), #809 merged 22:07 (out)
