# xvnej User-Interaction Findings — Small Improvements

**Date:** 2026-06-04
**Source:** xvnej (QA node) flight-recorder `xvn.db` (volume `xvnej_data`), interaction window **2026-06-02 → 2026-06-03** (predates the 2026-06-04 image).
**Method:** mined `api_audit` (299 ops / 83 errors), `auth_audit` (229 HTTP requests), `agent_runs`, `eval_runs`, `chat_messages`, `session_events`, `supervisor_notes` from real user activity.
**Scope note:** provider *credential/endpoint* problems (exhausted OpenRouter credits, dead `trycloudflare` quick-tunnel base URLs) are treated as operator/config error and are **out of scope** here — they appear below only as the evidence that motivates the preflight improvement (F1).

These are all small, self-contained improvements.

---

## F1 — Provider preflight gives a false green light

Preflight logs success and the run then fails on the *same* provider:

- Run `01KT3KAY…`: `preflight passed: 1 provider(s) verified reachable before launch (gemini-personal)` → trader failed `Not Found`.
- Run `01KT3KCA…`: `preflight passed … (openrouter)` → trader failed `Insufficient credits` (HTTP 402).
- Run `01KT3KKF…`: `preflight passed … (openrouter)` → trader failed `Provider returned error`.

Two gaps:
1. **Reachability ≠ usable.** A reachable endpoint can still 402 (no credits), 404 the model, or otherwise reject the call. Preflight should resolve the *exact* model the run will call and treat auth/billing/model-not-found as a preflight failure, not a pass.
2. **It checks only one provider.** "1 provider(s) verified reachable" is declared a pass even when the run depends on a specific bound provider/model. Preflight should verify every provider/model the strategy actually binds.

**Evidence:** `supervisor_notes` (preflight `info` rows immediately followed by `error` rows on the same `run_id`); `agent_runs.error`; `eval_runs` (all 3 = `failed`).

**Improvement:** make preflight predictive — per-bound-provider model resolution + auth/billing check — so a "pass" means the run can actually call its model.

---

## F2 — One invalid provider entry breaks the entire providers surface

`settings/providers.list` failed **72 of 90 calls over ~22 hours** (2026-06-02 11:06 → 2026-06-03 09:38) with:

```
validation: load config: validation failed for /data/config/default.toml:
providers[5].name: provider name must match [a-z0-9-]+
providers[7].name: provider name must match [a-z0-9-]+
```

A single malformed `name` (a capitalized `Gemini`) made whole-config validation reject the load, so **every** provider list/show returned an error and the user couldn't see or manage any provider. They were also blocked from re-adding it (`409 conflict: provider 'Gemini' already exists`). It was eventually resolved by a manual config edit, but the all-or-nothing failure mode remains.

**Evidence:** `api_audit` — `providers.list` 72 errors (80% of all API errors), `providers.add` 6 errors, `providers.show` 3 errors.

**Improvement:** validate providers per-entry. Return the valid providers with per-row error annotations on the bad ones, instead of failing the entire list/show. A user should never lose visibility of all providers because one entry is malformed.

---

## F3 — Users repeatedly fail to start evals (400) with no captured reason

`POST /api/eval/runs` returned **400 Bad Request ≥12 times** (plus 2× 422), in tight frustrated bursts — e.g. six attempts within ~30s at 2026-06-02 10:43. Only 3 attempts ever reached `202 Accepted` (and those 3 runs then failed at the trader stage, see F1).

These 400s are rejected at the HTTP/validation layer **before** the domain op runs, so `api_audit` shows `eval.start` with **0 errors** — the failures and their reasons are invisible in the domain audit (`auth_audit` only records route + status).

**Evidence:** `auth_audit` (`/api/eval/runs` POST: 400×12, 422×2, 202×3); `api_audit` `eval.start` n=3 / errs=0 (the mismatch).

**Improvements:**
1. Surface the validation reason to the client and into `api_audit` so eval-start failures are diagnosable.
2. The high 400 rate suggests the eval-start request contract is easy to get wrong from the dashboard — worth checking the form/payload that produces these and tightening client-side validation or the error message.

---

## F4 — Strategy route namespace / method confusion → 405s

A client hunted for the right way to mutate a strategy and hit 405 (Method Not Allowed) four times on the **singular** path:

- `PATCH /api/strategy/01KT3K0FZE…` → 405
- `PUT  /api/strategy/01KT3K0FZE…` → 405
- `POST /api/strategy/01KT3K0FZE…` → 405
- `GET  /api/strategy/01KT3K0FZE…/validate` → 405
- (also `PATCH /api/agents/01KT3K52…` → 405)

The working surface is the **plural** `/api/strategies` + the `update_metadata` op (`api_audit` shows `strategy.update_metadata` succeeding). The singular vs plural split and the unclear update verb cost the user multiple failed probes.

**Improvement:** unify on one namespace (plural), or have the singular path 404/405 with a "did you mean `/api/strategies/{id}`?" hint. You already do this well elsewhere — `strategy.get` returns `validation: id matches an agent; did you mean agents.get?` — so extend that helpfulness to route/method mismatches.

---

## F5 — Run status inconsistency: parent `failed`, child step `completed`

For each failed live run, the parent `agent_runs` row is `failed` (`step did not complete … role=trader`), but the child `…::trader::cycle0` row is `completed`. The sub-step status doesn't reflect that the step actually failed.

**Evidence:** `agent_runs` — e.g. `01KT3KAY…` = `failed` while `01KT3KAY…::trader::cycle0` = `completed`.

**Improvement:** propagate failure to the step row (or stop marking a step `completed` when its parent fails on that step). Otherwise step-level dashboards/queries over-report success.

---

## F6 — Config location is ambiguous (volume vs bind mount)

The app loads `/data/config/default.toml` (the `xvnej_data` volume), but the container also bind-mounts a separate `/config` (`→ /root/deploy/xvision/config`) whose contents diverge from the volume copy. Edits to the "obvious" bind-mounted dir wouldn't take effect on the running app.

**Improvement:** load from one well-documented location, or have `xvn doctor` print the effective config path it actually reads (so operators edit the right file).

---

## F7 — `full_debug` retention enabled on a shared QA node

Startup warns:

```
WARN xvision_observability: full_debug retention enabled. Prompts, responses,
and tool payloads may be stored on disk. Disable for shared / client work.
```

xvnej is a shared QA node, so prompts/responses/tool payloads are being persisted to disk for everyone using it.

**Improvement:** default shared/QA nodes to a lower retention mode, or make the warning actionable (point at the setting/route to change it).

---

## Positives observed

- The id-namespace error is genuinely helpful: `strategy.get` → *"id matches an agent; did you mean agents.get?"* — the model for the F4 improvement.
- Read/list surfaces (strategies, agents, eval runs, scenarios) were reliable — 0 errors across `strategy.list_paged` (26), `agents.list_paged` (24), `eval.get_run` (23), `eval.list_summaries_paged` (14).
- Preflight *does* run before launch and records structured notes — the bones are right; F1 is about making the check predictive rather than adding the concept.

## Prioritization

1. **F1** (predictive preflight) and **F2** (per-entry provider validation) — biggest user-visible reliability wins.
2. **F3** (eval-start error visibility) and **F4** (route/method hints) — diagnosability + fewer dead-end clicks.
3. **F5, F6, F7** — correctness/ergonomics cleanups.

## Artifacts

- Source DB snapshot: `/tmp/xvnej.db` (copy of `xvnej_data` volume `xvn.db`)
- Failed runs: `01KT3KAY9J8X08KH5ADBRZ3N88`, `01KT3KCA2MQGH3DT7P8QHJ53QZ`, `01KT3KKF2G88MRFM3PA3G8R1MP`
- Failed chat session: `01KT3HYM5RS9D37ZQJYJCZRMRC` (3× `session_failed`, 0 assistant replies)
