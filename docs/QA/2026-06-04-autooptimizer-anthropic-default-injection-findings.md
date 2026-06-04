# Hardcoded `anthropic` default-injection + image verification (F30, F22 correction)

**Date:** 2026-06-04
**Deploy under test:** `xvision:deploy-latest` (image built 13:57Z = 21:57 +0800) = PRs **#807 (F22) + #808 (F11)**. **#809 (F23) is NOT in this image** (merged 22:07 +0800, ~10 min after the build; confirmed: no `cycle_cost` table in the running DB).
**Findings only — no code.**

---

## F30 — [HIGH] ✅ RESOLVED (PR pending) `anthropic.claude-*` is hardcoded as a default where the operator chose openrouter

**Resolution (2026-06-04).** The example seeder no longer injects any `anthropic.claude-*`
literal. `crates/xvision-cli/src/commands/example/seed.rs` now binds the seeded trader
agent (and the manifest `attested_with` provenance, and the seeded `autooptimizer.toml`)
to shared `SEED_DEFAULT_PROVIDER`/`SEED_DEFAULT_MODEL` constants =
`openrouter` / `google/gemini-3.1-flash-lite` — the same registered combo the optimizer
config already used, so the optimizer config and the example trader can no longer drift.
A regression test (`seeded_example_trader_binds_to_registered_provider_not_anthropic`)
loads the seeded strategy + its scoped agent and asserts the slot resolves to the
registered provider with no `anthropic` in the binding or provenance. **Operational
note:** a node with the OLD on-disk legacy example (the drifted `anthropic.claude-sonnet-4.6`
form) is corrected by re-running `xvn example seed --reset`, which deletes the legacy
example and recreates it on the registered provider. The audit candidates
(`agents/store.rs`, `validator.rs`, `execute_cline.rs`, `recovery.rs`, `eval/review/*`)
were triaged and confirmed `#[cfg(test)]` fixtures / legitimate support — **no other
production default-injection site exists.** See also **F31** for the deeper mechanism.

The preflight guidance (F22) was also reworded to lead with "re-author the trader onto a
registered provider" and to reference F30, instead of misleadingly suggesting the
operator stand up the unregistered `anthropic`.

---

## F31 — [MED] ✅ RESOLVED (PR pending) `attested_with` (provenance metadata) masquerades as the operational binding

**Resolution (2026-06-04).** `LLMSlot::effective_model()`
(`crates/xvision-engine/src/strategies/slot.rs`) no longer falls back to
`attested_with`. An unset/blank `model` now resolves to an **empty string** (the slot
is *unbound*) instead of silently promoting the provenance string to the operational
model. Provenance can no longer become the binding: a model-less legacy `trader_slot`
whose `attested_with` was `anthropic.claude-sonnet-4.6` no longer dispatches anthropic.
New-schema agent slots are unaffected — `agent_slot_to_llm_slot` already sets `model`
explicitly whenever the agent has one, so only truly model-less legacy slots change
(operator-confirmed acceptable: "if they don't have an agent that's fine"). A new
`LLMSlot::has_model_binding()` makes the unbound state explicit, and the optimizer
preflight now rejects an unbound legacy trader with a clear "no model configured"
message rather than letting the cycle dispatch an empty model id. The `attested_with`
field doc and the `effective_model`/preflight doc comments were corrected to state
provenance is never the binding. Regression tests:
`effective_model_never_promotes_attested_provenance` (+ explicit-binding and
blank-model cases) in `slot.rs`; full engine lib suite (1010 tests) green.

---

### (original finding)

**Root mechanism behind F30 (operator-confirmed, 2026-06-04).** Nothing overwrites the
strategy at runtime — `anthropic` is baked into the strategy's *persisted* content and then
read as the operational binding via a metadata fallback. The exact chain for a legacy-schema
example (empty `agents[]`, populated legacy `trader_slot`):

1. `trader_slot` has `provider: None`, `model: None`, only `attested_with: "anthropic.claude-sonnet-4.6"`.
2. `LLMSlot::effective_model()` (`crates/xvision-engine/src/strategies/slot.rs:32`) falls back
   to `attested_with` when `model` is empty — so the model *becomes* `anthropic.claude-sonnet-4.6`.
3. `infer_trader_provider("", "anthropic.claude-sonnet-4.6")` parses the `anthropic.` prefix →
   provider `anthropic` → mismatch vs the openrouter cycle → fast-fail.

The contradiction: `LLMSlot.attested_with`'s own field doc (`slot.rs:7-11`) says it is
"Informational… **Never gates eval-launch** — the operator's `provider`+`model` binding is
authoritative." But `effective_model()` promotes it to the binding whenever explicit
`provider`/`model` are absent. So a slot with no real model silently "becomes" whatever its
provenance string says. F30 stops the *seeder* from persisting a bad provenance value, but the
masquerade itself remains a latent footgun for any legacy/hand-authored slot.

**Why not fixed in the F30 PR:** removing the `effective_model()` → `attested_with` fallback is a
resolver-semantics change with real blast radius — legacy on-disk strategies with model-less
`trader_slot`s currently rely on it to resolve *any* model at all; dropping it would leave them
unbound. The correct fix is a deliberate decision (a migration that promotes `attested_with` →
explicit `model` for legacy slots, or making model-less slots an explicit "unbound — needs a
provider" state rather than silently deriving one from provenance). Tracked here so it isn't lost.

**Acceptance (when taken on):** a model-less slot never derives its operational provider/model
from `attested_with`; legacy strategies are migrated to carry an explicit binding; a test asserts
provenance can never become the binding.

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
