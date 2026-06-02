# Internal Model-Facing Surfaces

**Created:** 2026-06-02  
**Purpose:** Operator reference for all strings/templates that are sent to
LLMs as system prompts, user messages, or tool descriptions — surfaces that
are otherwise invisible in the dashboard. Useful for debugging why a model
produces unexpected output.

---

## Surface index

| # | Name | File | Path type | Operator-visible? |
|---|------|------|-----------|-------------------|
| 1 | Cline user turn (`render_prompt`) | `execute_cline.rs:328` | Cline path only | No |
| 2 | Legacy user turn (`initial_user`) | `execute.rs:260` | Legacy (`LlmDispatch`) only | No |
| 3 | Anthropic schema preamble (`prompt_contract`) | `llm.rs:443` | Anthropic provider only | No |
| 4 | Malformed-JSON repair turn | `recovery.rs:506` | Legacy path only | Trace dock |
| 5 | Missing-field repair turn | `recovery.rs:813` | Legacy path only | Trace dock |
| 6 | Context-overflow summarize system prompt | `summarize.rs:80` | Legacy path, F-5 recovery | No |
| 7 | V2D memory recall block | `memory_recorder.rs:182` | Both paths (prepended to system) | No |
| 8 | Repeated-tool-failure result | `execute.rs:1031` | Legacy path only | No |
| 9 | Asset-mismatch tool error | `execute.rs:96` | Legacy path only | No |
| 10 | `indicator_panel` tool description | `tools/indicators.rs:26` | Legacy path only | No |
| 11 | `ohlcv` tool description | `tools/ohlcv.rs:27` | Legacy path only | No |
| 12 | Builtin agent template system prompts | `agents/templates.rs:69–587` | Pre-seeded starters | Dashboard (agent editor) |

---

## Detailed surfaces

### 1 — Cline path user turn (`render_prompt`)

**File:** `crates/xvision-engine/src/agent/execute_cline.rs:328–335`  
**Sent as:** First `step` prompt (user turn) to the Cline sidecar.  
**Which models see it:** Any model invoked via the Cline sidecar (`execute_slot_cline`).  
**Operator-visible:** No.

**Exact template:**
```
Inputs:
{upstream_inputs as pretty JSON}

Follow the slot's instructions. You may call tools to fetch additional data
for the current decision asset only; submit your final decision via the
`submit_decision` tool as JSON matching the required schema.
```

**Improvement opportunity:** The phrase "matching the required schema" is
vague — the schema name and fields are not inlined here (they are shipped
separately via `StartRunParams.decision_schema`). A model that ignores
`decision_schema` gets no field list in the prompt. Consider including the
schema name (e.g. `submit_decision({ action, conviction, justification })`)
to make the contract self-contained. Also note that Surface 2 (legacy) uses
"emit your final decision as JSON" while this surface uses "submit via
`submit_decision` tool" — the two paths give the model materially different
instructions for what to do with the output.

---

### 2 — Legacy path user turn (`initial_user`)

**File:** `crates/xvision-engine/src/agent/execute.rs:260–265`  
**Sent as:** First user message in the `LlmRequest.messages` array.  
**Which models see it:** Any model invoked via `execute_slot` (legacy `LlmDispatch` path — backtest/paper when Cline is not enabled).  
**Operator-visible:** No.

**Exact template:**
```
Inputs:
{upstream_inputs as pretty JSON}

Follow the slot's instructions. You may call tools to fetch additional data
for the current decision asset only; emit your final decision as JSON.
```

**Improvement opportunity:** "emit your final decision as JSON" says nothing
about schema shape or field names. The schema constraint arrives via a
separate `response_schema` field (Anthropic appends it to the system prompt;
OpenAI-compat uses `response_format`). A model that reads only the user turn
sees no JSON contract. Inlining the schema name or required fields would make
this turn self-consistent. Compare with Surface 3 which does enumerate fields.

---

### 3 — Anthropic schema preamble (`prompt_contract`)

**File:** `crates/xvision-engine/src/agent/llm.rs:443–449` (called from `llm.rs:701`)  
**Sent as:** Appended to the slot's `system_prompt` immediately before the Anthropic dispatch wire call. Not sent on OpenAI-compat paths (those use `response_format: json_schema` instead).  
**Which models see it:** Anthropic-provider models only, when the slot has a `ResponseSchema` (currently: `trader` role).  
**Operator-visible:** No.

**Exact template:**
```
\n\nYou must respond with exactly one JSON object matching this JSON Schema.
Do not include markdown, prose, or extra keys.
Schema `{schema.name}`:
{schema.schema as JSON}
```

For the trader slot the appended text is approximately:
```
You must respond with exactly one JSON object matching this JSON Schema.
Do not include markdown, prose, or extra keys.
Schema `trader_output`:
{"type":"object","required":["action","conviction","justification"],...}
```

**Improvement opportunity:** This text is appended after the operator's own
system prompt with no separator except `\n\n`. If the operator's prompt
already contains JSON output instructions (common in starter templates), the
model receives two conflicting output contracts. A clearly labelled separator
(`--- Output contract (enforced by harness) ---`) would prevent confusion.
Also, this block is Anthropic-only; OpenAI-compat models receive the schema
via `response_format` silently — inconsistent operator experience when
switching providers.

---

### 4 — Malformed-JSON repair user turn

**File:** `crates/xvision-engine/src/agent/recovery.rs:506–514`  
**Function:** `build_malformed_json_repair_message(parse_error, schema)`  
**Sent as:** Third user turn in a repair-attempt `LlmRequest` (after original user prompt → malformed assistant response → this repair message).  
**Which models see it:** Legacy path only, when `TraderOutputError::InvalidJson` or `TraderOutputError::Truncated` fires.  
**Operator-visible:** Visible in the trace dock as a recovery attempt span.

**Exact template:**
```
Your previous response failed to parse: {parse_error}

Emit a single JSON object matching the `{schema.name}` schema below.
Do not include prose, code fences, or tool calls. Return ONLY the JSON object.

Schema:
{schema.schema as pretty JSON}
```

**Improvement opportunity:** The repair turn explicitly removes tools ("do
not include … tool calls") but does not confirm what `{parse_error}` means
— a raw `serde_json` error like `expected `,` or `}` at line 3 column 7`
may not help the model fix the right thing. Consider mapping common parse
error patterns to human-readable hints (e.g. "unmatched brace", "trailing
comma", "truncated object").

---

### 5 — Missing-field repair user turn

**File:** `crates/xvision-engine/src/agent/recovery.rs:813–821`  
**Function:** `build_schema_missing_field_repair_message(problem_fields, parse_error)`  
**Sent as:** Third user turn in a targeted-patch repair attempt.  
**Which models see it:** Legacy path only, when `TraderOutputError::MissingField` or `TraderOutputError::InvalidField` fires.  
**Operator-visible:** Visible in the trace dock as a recovery attempt span.

**Exact template:**
```
Your previous response was missing or invalid for the following fields: [{fields}].

Re-emit ONLY a single JSON object containing those fields, filled in correctly.
The other fields you produced are accepted as-is — do not repeat them.
Do not include prose, code fences, or tool calls.

Validator detail: {parse_error}
```

**Improvement opportunity:** "Re-emit ONLY a single JSON object containing
those fields … The other fields you produced are accepted as-is — do not
repeat them" asks the model to produce a *partial* patch. Several models
(especially instruction-tuned ones expecting complete objects) still emit
the full schema anyway — the harness merges via `merge_and_reparse_trader_output`
and that is the intentional path, but the instruction can cause a second
parse failure if the model emits a valid-but-different extra field. Consider
adding a sentence confirming that extra fields in the re-emit are harmless
and will be ignored.

---

### 6 — Context-overflow summarize system prompt

**File:** `crates/xvision-engine/src/agent/summarize.rs:80–89`  
**Constant:** `SUMMARIZE_SYSTEM_PROMPT`  
**Sent as:** `system_prompt` of a separate cheap-model `LlmRequest` during F-5 context-overflow recovery.  
**Which models see it:** Legacy path only; the cheapest priced model in the configured catalog (selected by `summarize::pick_cheap_model`). Cline path has no equivalent recovery.  
**Operator-visible:** No.

**Exact text:**
```
You are summarizing the middle of a trading-agent conversation so it can be
re-fed under a tighter context budget. Constraints:
- PRESERVE: proper names (symbols, model ids, broker ids, tool names),
  numeric quantities (prices, sizes, percentages, dates), and explicit
  risk constraints (caps, max-drawdown, leverage).
- DROP: chain-of-thought, hedging language, restated prompts, pleasantries.
- LENGTH: ≤ 1500 tokens. Prefer concise factual bullet points over prose.
- FORMAT: start with one line `[history summarized]`, then bullets. Do not
  fabricate facts not present in the source.
```

**Improvement opportunity:** Hardcoded for v1 (the module doc explicitly
notes this). If the cheap model is not a trading-domain model (e.g. a
general-purpose mini model), the domain vocabulary in PRESERVE may not be
recognised and summaries may silently drop critical numeric context. An
operator currently has no way to know this recovery path fired or which
model handled it. Consider emitting an `UnifiedEvent` observability span
when this path triggers.

---

### 7 — V2D memory recall block

**File:** `crates/xvision-engine/src/agent/memory_recorder.rs:182–192`  
**Function:** `render_recalled_patterns(matches)`  
**Sent as:** Prepended to `assembled_system_prompt` before every dispatch when V2D memory mode is active and the recall returns hits (`execute.rs:381–383`).  
**Which models see it:** Both legacy and Cline paths (the assembly happens before the slot dispatches in either direction).  
**Operator-visible:** No.

**Exact template:**
```xml
<prior_observations>
A prior decision noted: "{text_preview}". Consider whether this
situation matches the present cycle.

[... repeated for each recalled pattern ...]
</prior_observations>
```

**Improvement opportunity:** The `<prior_observations>` XML block is
prepended before the operator's own system prompt content. Models may treat
it as a role instruction rather than a recall hint, especially if the
operator's prompt also starts with a persona statement ("You are a trader…").
The block has no explicit instruction about precedence (ignore if irrelevant,
use as background context, etc.). Also: the text preview is truncated by the
`preview()` helper but the truncation boundary is not visible to the model.

---

### 8 — Repeated-tool-failure injected result

**File:** `crates/xvision-engine/src/agent/execute.rs:1031–1038`  
**Function:** `repeated_tool_failure_result(tool_name)`  
**Sent as:** `ContentBlock::ToolResult` with `is_error: Some(true)` injected into the message history, replacing the real tool call on the 3rd+ identical failure.  
**Which models see it:** Legacy path only.  
**Operator-visible:** No.

**Exact template:**
```
repeated_tool_failure: tool '{tool_name}' with this exact input has failed
{MAX_TOOL_RETRIES_PER_PAIR} times in this slot execution. The input is
blocked for the remainder of this run. Retry with a different input or
choose a different tool.
```

(`MAX_TOOL_RETRIES_PER_PAIR` is currently 2.)

**Improvement opportunity:** The message tells the model to "retry with a
different input" but gives no hint about what change would be valid. Models
frequently respond by re-issuing the same tool with minor cosmetic changes
(e.g. changing whitespace in a JSON field) that hash-collide differently
but logically fail the same way. Consider including the blocked input hash
or a one-line summary of why it failed.

---

### 9 — Asset-mismatch tool error

**File:** `crates/xvision-engine/src/agent/execute.rs:96–100`  
**Function:** `market_data_tool_asset_mismatch(tool_name, tool_input, decision_asset)`  
**Sent as:** `ContentBlock::ToolResult` with `is_error: Some(true)` when the model requests market data for an asset other than the current decision cycle's asset.  
**Which models see it:** Legacy path only; fires for `ohlcv` and `indicator_panel` tools.  
**Operator-visible:** No.

**Exact template:**
```
tool error: asset mismatch for {tool_name}: current decision asset is
{decision_asset} but tool requested {requested_asset}. Use the current
decision asset only; do not fetch cross-asset market data for this
per-asset decision.
```

**Improvement opportunity:** Reasonable and clear. Minor: the normalisation
used for comparison (`normalize_asset_for_compare`) strips USD suffix and
upper-cases, so BTC/USD and BTCUSD compare equal. A model that requests
`btc-usd` might still see a mismatch even though the intent is correct.
Consider logging the normalised pair in the error message to help operators
distinguish true mismatches from normalisation edge cases.

---

### 10 — `indicator_panel` tool description

**File:** `crates/xvision-engine/src/tools/indicators.rs:26–28`  
**Sent as:** `ToolDefinition.description` advertised to the model in every legacy-path dispatch where `indicator_panel` is in `slot.allowed_tools`.  
**Which models see it:** Legacy path only.  
**Operator-visible:** No.

**Exact text:**
```
Computed indicator panel (RSI, MACD, BB, ATR, MA, EMA)
```

**Improvement opportunity:** Extremely terse. The model has no information
about required input fields (`asset`, `fixture`, `lookback_bars`) from the
description alone — it must guess or fail. No mention of output shape (what
the returned panel looks like). A richer description with the required input
schema and return type would reduce tool-call parse errors.

---

### 11 — `ohlcv` tool description

**File:** `crates/xvision-engine/src/tools/ohlcv.rs:27–29`  
**Sent as:** `ToolDefinition.description` advertised to the model where `ohlcv` is in `slot.allowed_tools`.  
**Which models see it:** Legacy path only.  
**Operator-visible:** No.

**Exact text:**
```
OHLCV history for an asset and time range
```

**Improvement opportunity:** Mentions "time range" but the tool currently
requires a `fixture` name (not a real date range — live Alpaca fetch is
deferred). A model that passes `start_time`/`end_time` parameters gets an
error. The description should note that `fixture` is a named dataset
(backtest use only) and `lookback_bars` controls depth, not a wall-clock
range. Misleading description is a frequent source of `ohlcv` tool-call
failures in backtest traces.

---

### 12 — Builtin agent template system prompts

**File:** `crates/xvision-engine/src/agents/templates.rs:69–587`  
**Sent as:** Pre-seeded `AgentSlot.system_prompt` values shown in the agent editor when an operator picks a starter template. These become the actual system prompt sent to the model once the operator saves without modifying them.  
**Which models see it:** Both paths; depends on what the operator has configured.  
**Operator-visible:** Yes — visible and editable in the dashboard's agent editor. However, operators often save templates unchanged.

The 9 templates and their slot prompts (abbreviated — see source for verbatim text):

| Template id | Slot(s) | Opening line |
|---|---|---|
| `single-trader` | `main` | "You are a discretionary trader making one decision per cycle…" |
| `analyst-executor` | `analyst` | "You are a market analyst. Read the briefing and output a structured thesis…" |
| `analyst-executor` | `executor` | "You are an executor. Given the analyst's thesis, output a single JSON decision…" |
| `risk-checked-trader` | `trader` | "You are a trader. Propose a decision given the briefing…" |
| `risk-checked-trader` | `risk_check` | "You are a risk gate. Given the trader's proposed decision and the current portfolio state…" |
| `risk-checked-trader` | `executor` | "You are an executor. Given the trader's decision and the risk gate's verdict…" |
| `momentum-trader-only` | `trader` | "You are a momentum trader. Read the briefing…only open positions that align with the dominant short-to-medium-term trend…" |
| `mean-reversion-trader` | `trader` | "You are a mean-reversion trader…only open positions when price has stretched meaningfully away from a reference mean…" |
| `multi-asset-router-with-traders` | `router`, `equities_trader`, `crypto_trader`, `fx_trader` | Various |
| `regime-aware-trader` | `regime`, `trader` | "You are a regime classifier…", "You are a regime-aware trader…" |
| `news-reader-plus-trader` | `news`, `trader` | "You are a news reader…", "You are a trader who consumes the news reader's digest…" |
| `paper-confirmed-live-trader` | `paper_trader`, `executor` | "You are a paper trader…", "You are a live executor…" |

**Improvement opportunity:** All 9 templates instruct the model to "Output
exactly one JSON object matching: {…inline schema…}" with the schema inlined
as a literal string (e.g. `{"action":"long_open|short_open|flat|hold",
"conviction":0..1, "justification":"string"}`). This inline schema is a
partial description (no `required`, no type constraints) that diverges from
the engine's actual `ResponseSchema::trader_output()` contract. If Anthropic's
schema preamble (Surface 3) appends a stricter schema, the model sees two
conflicting contracts — the template's inline schema and the formal JSON
Schema. This is a known source of model confusion when switching from
OpenAI-compat providers (where the preamble fires differently) to Anthropic.

---

## Cross-cutting notes

### Legacy vs. Cline path divergence

The legacy `execute_slot` path (Surface 2) and Cline `execute_slot_cline`
path (Surface 1) use different instructions for the same task:

| Aspect | Legacy (Surface 2) | Cline (Surface 1) |
|---|---|---|
| Output instruction | "emit your final decision as JSON" | "submit your final decision via the `submit_decision` tool" |
| Schema delivery | Appended to system prompt (Anthropic) or `response_format` (OAI-compat) | `decision_schema` in `StartRunParams` |
| Repair path | Surfaces 4 + 5 available | No repair path |
| Tool descriptions | Sent via `ToolDefinition.description` | Sidecar-managed |

A strategy that runs on both paths (e.g. during A/B compare) may produce
different output formats because the model receives different instructions.

### Provider asymmetry in schema delivery

The Anthropic dispatcher injects the schema as a system-prompt suffix
(Surface 3). OpenAI-compat dispatchers use `response_format: json_schema`
silently. Operators switching providers may see different model behaviour
because the schema "prompt" the model receives changes fundamentally — on
Anthropic it is natural language appended to the system block; on OpenAI it
is a machine-readable format field. Neither surface is visible in the
operator dashboard.
