# Intake — 2026-05-19 — QA: `validate_draft` false-positive on numbered prompts + silent failure loop

Operator finding (Ed, 2026-05-19) from a live chat-rail session that got
stuck in a 6-retry loop on `validate_draft` after creating a custom
strategy. The chat assistant kept calling `update_slot` → `validate_draft`
→ "Validation failed (1 error)" with no resolution and no visible error
text, until the operator gave up.

## Source

- Chat session: strategy `01KRZ0ZWER9HE2CTYNWT83ESYQ`
  ("Multi-Factor Logic Agent"), 2026-05-19 ~02:27Z, running against
  the `xvn-app` container on extndly-dev
  (image `xvision:deploy-latest`).
- Reconstructed transcript pasted into the QA chat (intake author has
  the full text).
- Live DB: `docker cp xvn-app:/data/xvn.db …` —
  `/data/strategies/01KRZ0ZWER9HE2CTYNWT83ESYQ.json` contains the
  draft as last persisted.
- Code: `crates/xvision-engine/src/strategies/validate.rs`,
  `crates/xvision-dashboard/src/wizard_loop.rs`.

## Already in flight (do not respawn)

- `harness-recovery-state-machine` (F-5 in
  `2026-05-18-harness-observability-audit.md`) is the *engine*-side
  recovery loop for agent runs. The wizard chat loop in
  `crates/xvision-dashboard/src/wizard_loop.rs` is a *separate*
  feedback loop — F-3 below covers it directly.
- `agent-error-feedback-self-healing` (round-3) covers broker-error
  surfacing in eval cycles, not the chat-rail tool surface.

## V2 roadmap items (not contracts here)

- V2A "ease of use" anchor: chat-rail must explain its own failures.
  This intake feeds that bucket.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| F-1 | P1 | `validate_prompt_manifest_alignment` extracts cadences with a "number followed by word starting with 'h' or 'm'" heuristic (`crates/xvision-engine/src/strategies/validate.rs:159-167`). English numbered lists collide catastrophically: `"3. Mean Reversion:"` tokenizes as `3` + `mean`, which `starts_with('m')` matches → validator emits *"prompt mentions 3m but manifest decision_cadence_minutes is 240m"*. No edit to the trader prompt can fix it short of removing the list. | `validator-cadence-parse-strict-units` |
| F-2 | P1 | `rich_block_for_tool_result` (`crates/xvision-dashboard/src/wizard_loop.rs:1464`) has no case for `validate_draft`, so the chat UI only shows the bare summary `"Validation failed (1 error)"` with the error text hidden. The operator can see *that* it failed but not *why*. | `chat-rail-validate-draft-rich-error` |
| F-3 | P2 | Chat wizard loop has no convergence guard on `validate_draft` — operator's session shows 6 consecutive `update_slot(trader.prompt)` + `validate_draft` cycles, each producing the same error class. The model has the error text in its tool_result context (`wizard_loop.rs:396-400`) but still retries the same fix shape. Needs (a) explicit instruction in the wizard system prompt to surface validate_draft errors verbatim to the user when a retry produces the same error, and (b) a hard retry budget per-error-class with an automatic user-visible escalation. | `chat-rail-validate-retry-budget` |
| F-4 | P2 | Eval `01KRZ18JTMZ1S7W1MBKC1PNNSJ` (started right after the strategy was finally validated) logged 44+ `"recoverable broker error fed back to agent for next cycle"` with `error_class=broker_min_order_size` (`xvision_engine::eval::executor::paper`). Suggests the wizard's `set_risk_config(preset=balanced)` defaults yield position sizes below the broker minimum for an ETH paper run — the self-healing loop spins but never finds a sizeable order. Worth verifying preset math against current broker mins. | `risk-preset-balanced-min-order-sanity` |
| F-5 | P3 | Same eval also logged `WARN xvision_engine::eval::postprocess: findings postprocess failed (run still ok) error=OpenAI-compat API error 400 … "claude-haiku-4-5-20251001 is not a valid model ID"`. The findings post-processor is routing an Anthropic model id through OpenRouter (`/v1/chat/completions`). Either swap to the Anthropic provider or map to OpenRouter's `anthropic/claude-haiku-4.5` slug. | `findings-postprocess-provider-routing` |

Five tracks. F-1 + F-2 together are the operator-visible failure (false
positive + can't see what failed); F-3 is the loop-control fix that
keeps a future false-positive from being silent. F-4/F-5 are independent
sightings from the same logs.

## Track summaries

### F-1 `validator-cadence-parse-strict-units` (P1, leaf, S)

The cadence parser at
`crates/xvision-engine/src/strategies/validate.rs:144-170` uses two
permissive heuristics:

1. `cadence_word_minutes` strips single-char `'h'` / `'m'` suffixes, so
   `"5m"` / `"1h"` work. Fine on its own.
2. The cross-word check splits on whitespace and accepts any pair where
   the next word's `.starts_with('h')` or `.starts_with('m')` is true.
   This is what collides with `"3. Mean Reversion"` → cadence "3m".

Fix: tighten the cross-word check so only an exact unit word counts —
`"hour"`, `"hours"`, `"minute"`, `"minutes"`, `"h"`, `"m"` as the full
trimmed token, not `starts_with`. Add a regression test using the
exact failing prompt:

```text
1. Trend: ...
2. Conviction: ...
3. Mean Reversion: ...
```

with manifest cadence 240. Today the test would catch the false
positive on `3 + mean`.

Adjacent: `mentioned_assets` (same file, line 130) uses a similar
loose token scan that probably has analogous false positives on
slashes in non-asset contexts (e.g. URLs in a prompt). Out of scope
for this track unless an operator hits it.

### F-2 `chat-rail-validate-draft-rich-error` (P1, leaf, S)

Add a `validate_draft` arm to
`rich_block_for_tool_result` (`crates/xvision-dashboard/src/wizard_loop.rs:1468`)
that renders an inline error card when `result.ok == false`:

- Title: "Validation failed"
- Body: bullet list of `result.errors[]` verbatim
- Action: "Open draft" → `/authoring/<id>`

Must respect the no-popups rule (`xvision/CLAUDE.md` → "Frontend UI
rule: no popups") — inline expanded card in the chat rail, not a
modal. Pattern mirrors the existing `create_strategy` /
`run_eval` cards in that same function.

This is the smallest change that would have unblocked the operator —
they could have read "prompt mentions 3m but manifest is 240m" and
either fixed the prompt themselves or filed F-1 immediately.

### F-3 `chat-rail-validate-retry-budget` (P2, leaf, M)

Two changes:

1. **System prompt** (`crates/xvision-dashboard/src/wizard_loop.rs`
   wizard prompt — search for the existing `"Do not say a tool change
   succeeded until the tool_result says it succeeded"` line, add
   adjacent): *"When `validate_draft` returns `ok: false`, quote the
   `errors[]` to the user before attempting a fix. If the same error
   class appears across two consecutive `validate_draft` calls,
   stop editing and ask the user — do not silently retry."*
2. **Loop guard**: track `(error_class, count)` in `WizardLoop`. After
   2 `validate_draft` failures with overlapping error text, force a
   final `EndTurn` with the error surfaced as a user-visible
   `WizardEvent::ContentBlock`. Same conceptual shape as
   `last_tool_error` already tracked at line 384.

Adjacent: the broader pattern (chat agents looping on a tool error
they can't see resolved) is also called out in F-5 of the
harness-observability audit — different surface (eval runner vs.
chat wizard), same anti-pattern. Coordinate the recovery vocabulary
so both surfaces use the same error-class names where possible.

### F-4 `risk-preset-balanced-min-order-sanity` (P2, integration, M)

Verify that `set_risk_config(preset=balanced)` (which the operator
used) actually produces orders above the paper broker's per-asset
minimum for an ETH-only universe with a typical starting balance.
The 44+ consecutive `broker_min_order_size` warnings on
`run_id=01KRZ18JTMZ1S7W1MBKC1PNNSJ` say it doesn't, at least not for
this strategy's risk_pct/leverage combo (`0.015` per trade,
`max_leverage=3`, balance per default).

Either:
- raise the default starting balance for paper runs, or
- bump `balanced.risk_pct_per_trade` floor for low-leverage configs,
  or
- have the executor convert "below min" to a single user-visible
  surface error after N consecutive failures instead of looping
  silently (the self-healing path is doing its job — the issue is
  that "min order size" is not a self-healable failure mode for a
  fixed-config run).

### F-5 `findings-postprocess-provider-routing` (P3, leaf, S)

`xvision_engine::eval::postprocess` is calling the OpenRouter base
URL with the Anthropic model id `claude-haiku-4-5-20251001`. Two
sane fixes:

- Route findings-postprocess through the Anthropic provider directly
  (same model id), or
- If OpenRouter is the intended provider, translate to
  `anthropic/claude-haiku-4.5` (the OpenRouter slug) at the
  dispatch boundary.

Currently classified as `"findings postprocess failed (run still ok)"`
— non-blocking but the findings panel will be empty for any run that
hits this path.

## Audit detail

Verbatim slice from `docker logs xvn-app --since 24h` for context:

```
2026-05-19T02:33:02Z WARN xvision_engine::eval::executor::paper:
  recoverable broker error fed back to agent for next cycle
  run_id=01KRZ18JTMZ1S7W1MBKC1PNNSJ decision_index=1
  error_class="broker_min_order_size" n_recoverable=1
… [40+ identical lines through decision_index=44] …
2026-05-19T02:34:23Z WARN xvision::llm:
  OpenAI-compat API returned non-success
  provider="openai-compat" url=https://openrouter.ai/api/v1/chat/completions
  status=400 Bad Request
  body={"error":{"message":"claude-haiku-4-5-20251001 is not a valid model ID",…}}
```

The validation-loop chat itself does **not** appear in the engine
logs — the chat-rail loop happens in `xvision-dashboard` with the
errors only flowing through `tool_result` JSON. F-2 is partly an
observability gap as well: tracing should record validate_draft
failures with their error text so this kind of session is
post-mortem-able from logs alone.

## Sequencing

1. F-1 first (one-line parser tighten + regression test). Stops the
   false positive at the source.
2. F-2 in parallel (different file, frontend-adjacent). Even after F-1
   lands, real validation errors should be visible.
3. F-3 after F-2 — guard depends on F-2's surfacing path.
4. F-4 / F-5 independent; either can land any time.

F-1 + F-2 together are the operator-actionable fix; everything else
is hardening.
