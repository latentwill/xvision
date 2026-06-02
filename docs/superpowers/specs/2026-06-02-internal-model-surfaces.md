# Internal Model Surfaces Catalog

**Date:** 2026-06-02  
**Status:** Reference  
**Audience:** Operators and developers debugging model failures

This catalog documents every string or template the harness sends to an LLM as a system prompt, user message, or tool result. Each entry gives the exact file:line, the exact text, which execution path sends it, whether it is operator-visible, and an improvement note.

---

## Execution paths

| Label | Description |
|---|---|
| **Stage 1** | Intern briefing (`xvision-intern`); talks directly to `InternBackend` — never goes through the Cline sidecar |
| **Legacy** | `execute_slot` (`execute.rs`); raw HTTP via `LlmDispatch` |
| **Cline** | `execute_slot_cline` (`execute_cline.rs`); routes through the `xvision-agentd` TypeScript sidecar |

---

## Surface 1 — Stage 1 intern system preamble

**File:** `crates/xvision-intern/src/prompt.rs:173` (`SYSTEM_PREAMBLE`)  
**Path:** Stage 1 only  
**Operator-visible:** No — hardcoded constant

Exact text:

```
You are a senior market analyst writing a balanced briefing for the trading desk.
Your single job is to surface the strongest case for each of {bull, bear, flat} given the data below, with supporting evidence tags.

HARD RULES — violation will fail downstream parsing:
1. You MUST NOT recommend a direction. Do NOT include a `candidate_direction` field.
   Do NOT lean toward one case in any of the case strings. Each case must read as if its author believed it.
2. Output JSON only. No prose, no commentary, no markdown fences.
3. Schema is fixed. Extra fields will be rejected.
4. `bull_case`, `bear_case`, `flat_case` MUST each be 20-2000 characters.
```

`build_intern_prompt()` pushes this constant first, then appends the dynamic market context, then appends `SCHEMA_INSTRUCTIONS` (Surface 2). The full assembled string is sent as the `user` content on both OpenAI-compat and Anthropic backends.

**Improvement opportunity:** "Output JSON only" matches the sidecar's `CONFLICTING_JSON_RE` regex (Surface 7). If Stage 1 were ever routed through the Cline path the sidecar would append `CORRECTION_NOTE` — a latent conflict. Currently unreachable, but worth knowing.

---

## Surface 2 — Stage 1 intern output schema instructions

**File:** `crates/xvision-intern/src/prompt.rs:183` (`SCHEMA_INSTRUCTIONS`)  
**Path:** Stage 1 only  
**Operator-visible:** No

Exact text:

```
# Required output (JSON only)
```
{
  "bull_case":  string  // 20-2000 chars; strongest bullish thesis given the data
  "bear_case":  string  // 20-2000 chars; strongest bearish thesis given the data
  "flat_case":  string  // 20-2000 chars; strongest no-trade thesis given the data
  "evidence_long":  [ {"kind": "technical|onchain|macro|sentiment|fundamental", "detail": "<short tag>"} ]
  "evidence_short": [ {"kind": "technical|onchain|macro|sentiment|fundamental", "detail": "<short tag>"} ]
  "evidence_flat":  [ {"kind": "technical|onchain|macro|sentiment|fundamental", "detail": "<short tag>"} ]
  "signal_quality": number  // 0.0-1.0; how confident you are that the data supports any meaningful read
}
```

Emit only the JSON object. The `cycle_id`, `asset`, `regime`, and `horizon_hours` fields are filled in by the runtime — do not include them.
```

**Improvement opportunity:** Uses a pseudo-JSON schema with inline comments (`string  // description`) inside a markdown code fence, rather than a formal JSON Schema object. Some models may include comment text or misinterpret field types. `ResponseSchema::trader_output()` (Surface 17) uses proper JSON Schema and is more reliable.

---

## Surface 3 — Stage 1 intern backend-level system prompt

**File:** `crates/xvision-intern/src/backend.rs:214` (OpenAI-compat), `backend.rs:292` (Anthropic)  
**Path:** Stage 1 only  
**Operator-visible:** No

Exact text (same on both backends):

```
You output only valid JSON conforming to the schema.
```

Sent as the `system` field on every `InternBackend::brief()` call. The user-facing market context and schema (Surfaces 1 + 2) go in the `user` turn.

**Improvement opportunity:** This terse system prompt competes with the far more detailed instructions in the user turn (Surfaces 1 + 2). Models that weight the system prompt more heavily may follow this minimally and ignore the hard rules in Surface 1. The rules in Surface 1 could be moved to the system prompt entirely for clarity.

---

## Surface 4 — Legacy path initial user turn

**File:** `crates/xvision-engine/src/agent/execute.rs:260`  
**Function:** `execute_slot()`  
**Path:** Legacy only  
**Operator-visible:** No (the briefing JSON is operator-data; the wrapper text is hardcoded)

Exact template (line 260–265):

```
Inputs:
{serde_json::to_string_pretty(&inputs_for_prompt)}

Follow the slot's instructions. You may call tools to fetch additional data for the current decision asset only; emit your final decision as JSON.
```

This is the first `role=user` message. Sent as a single `ContentBlock::Text` at line 311.

**Improvement opportunity:** "emit your final decision as JSON" is deliberately vague — it does not name the schema. This is intentional (the formal `ResponseSchema` is sent via the provider's structured-output mechanism), but a model that ignores the structured-output constraint will emit an arbitrarily-shaped blob. The Cline path (Surface 5) is explicit about `submit_decision`. Consider aligning the wording.

---

## Surface 5 — Cline path initial user turn (`render_prompt`)

**File:** `crates/xvision-engine/src/agent/execute_cline.rs:327`  
**Method:** `ClineSlotInput::render_prompt()`  
**Path:** Cline only  
**Operator-visible:** No

Exact template (lines 328–337):

```
Inputs:
{serde_json::to_string_pretty(&self.upstream_inputs)}

Follow the slot instructions. You may call tools to fetch additional data for the current decision asset only. You MUST reply by calling the submit_decision tool with your decision as a JSON argument. Do NOT output prose, raw JSON, or code fences in your reply — only the submit_decision tool call is accepted. Outputting text instead of calling the tool will fail the cycle.
```

Passed as `StepParams.prompt` to `AgentClient::step()`. Also reused verbatim as `initial_user_body` in the two recovery helpers in `recovery.rs` (lines 685 and 930).

**Improvement opportunity:** "Do NOT output prose, raw JSON, or code fences" is a negative instruction stack that can trigger some models' self-reference behavior. Monitoring for models that echo this prohibition back in their output is worthwhile.

---

## Surface 6 — Cline no-decision recovery prompt

**File:** `crates/xvision-engine/src/agent/execute_cline.rs:561`  
**Function:** `try_nodecision_recovery()` (lines 535–581)  
**Path:** Cline only (second `step()` call; session still open)  
**Operator-visible:** No

Exact text (lines 561–564):

```
You completed without calling submit_decision. Call submit_decision now with your decision as a JSON argument — do not output prose.
```

**When triggered:** After the first step completes with `status=completed` and `decision_json` is `None`, AND the `output_text` did not start with `{` (which would allow Method 1 recovery — adopting the prose JSON directly).

**Improvement opportunity:** This prompt does not re-state the inputs or the decision schema. A model that completed without calling `submit_decision` because it was uncertain about the format will receive the repair prompt without the context it needs. Adding the schema name (e.g. `{"action": "long_open|short_open|flat|hold", "conviction": 0..1, "justification": "..."}`) would reduce second-failure rates.

---

## Surface 7 — Sidecar system prompt sanitizer (`sanitizeSystemPrompt`)

**File:** `xvision-agentd/src/session/build-agent.ts:4`  
**Function:** `sanitizeSystemPrompt()` (lines 12–15); called at line 51 of `buildAgent()`  
**Path:** Cline only  
**Operator-visible:** No — applied silently; no dashboard indicator when it fires

Trigger regex (line 4–6):

```javascript
const CONFLICTING_JSON_RE =
  /output[^\n]*json[^\n]*only|strict\s+json|json\s+only|output\s+json/i
```

Appended text when triggered (`CORRECTION_NOTE`, line 8):

```
\n\nIMPORTANT: Ignore any earlier instructions to output raw JSON. You MUST call the submit_decision tool to submit your decision — outputting JSON text is not accepted and will fail the cycle.
```

Idempotency guard — skips append if `CORRECTION_MARKER` is already present (line 10):

```
You MUST call the submit_decision tool
```

**What triggers it:** Operator-authored slot prompts containing `output json`, `json only`, `output * json * only` (same line), or `strict json` (case-insensitive). The built-in templates (Surface 16) generally do NOT trigger it because their phrasing is "output exactly one JSON object" (no trailing "only").

**Improvement opportunity:** The appended "Ignore any earlier instructions" is an adversarial injection mid-prompt. Some models weight it as an override and suppress all JSON output, yielding `NoDecision` failures that then trigger Surface 6. The regex could be tightened to only true conflicts (e.g. `\bjson\s+only\b` at end of sentence). Operators currently have no visibility this fired — a structured log at `event=system_prompt_sanitized` would help debugging.

---

## Surface 8 — MalformedJson repair user message

**File:** `crates/xvision-engine/src/agent/recovery.rs:547`  
**Function:** `build_malformed_json_repair_message()` (lines 547–566); dispatched from `try_repair_malformed_json()` (line 706)  
**Path:** Legacy only — paper.rs / backtest.rs trigger this after `TraderOutput::parse_response` returns `InvalidJson` or `Truncated`  
**Operator-visible:** No (visible in trace dock as `recovery.attempt` spans)

Exact template (lines 557–565):

```
Your previous response failed to parse: {parse_error}

Emit a single JSON object matching the `{schema_name}` schema below. Do not include prose, code fences, or tool calls. Return ONLY the JSON object.

Schema:
{schema_body_pretty_json}
```

Three-turn conversation built:
1. `user`: the original user turn (Surface 4 text, reconstructed from `seed_inputs`, lines 685–694)
2. `assistant`: verbatim raw text that failed to parse
3. `user`: the repair message above

Repair dispatch strips all tools (`tools: Vec::new()` at line 737) — the model cannot use tool calls on this attempt.

**Improvement opportunity:** "Do not include … tool calls" is correct for the legacy path. If the repair path is extended to Cline, this instruction must change (Cline requires `submit_decision`). The `schema_name` is always `trader_output` — non-trader slots (critic, filter, router) would need a different schema if repair were extended to them.

---

## Surface 9 — SchemaMissingField repair user message

**File:** `crates/xvision-engine/src/agent/recovery.rs:859`  
**Function:** `build_schema_missing_field_repair_message()` (lines 859–878); dispatched from `try_repair_schema_missing_field()` (line 943)  
**Path:** Legacy only — triggered when `TraderOutputError.kind` is `MissingField` or `InvalidField`  
**Operator-visible:** No (visible in trace dock as `recovery.attempt` spans)

Exact template (lines 869–877):

```
Your previous response was missing or invalid for the following fields: [{fields}].

Re-emit ONLY a single JSON object containing those fields, filled in correctly. The other fields you produced are accepted as-is — do not repeat them. Do not include prose, code fences, or tool calls.

Validator detail: {parse_error}
```

Three-turn conversation built identically to Surface 8 (original user turn, assistant raw text, repair user turn). Repair dispatch also strips tools (line 971).

**Improvement opportunity:** "The other fields you produced are accepted as-is — do not repeat them" implies the original response was otherwise well-formed JSON. If the original JSON was partially corrupt (e.g. truncated values), the merge (`merge_and_reparse_trader_output`) may still fail after patching. The error message could clarify: "emit ONLY the specific fields listed; the engine will merge your patch over the original."

---

## Surface 10 — Context overflow summarize system prompt

**File:** `crates/xvision-engine/src/agent/summarize.rs:80` (`SUMMARIZE_SYSTEM_PROMPT`)  
**Path:** Legacy only — triggered by `FailureClass::ContextOverflow`, dispatched through `try_context_overflow_recovery()` using the cheapest catalog model  
**Operator-visible:** No

Exact text (lines 80–89):

```
You are summarizing the middle of a trading-agent conversation so it can be re-fed under a tighter context budget. Constraints:
- PRESERVE: proper names (symbols, model ids, broker ids, tool names), numeric quantities (prices, sizes, percentages, dates), and explicit risk constraints (caps, max-drawdown, leverage).
- DROP: chain-of-thought, hedging language, restated prompts, pleasantries.
- LENGTH: ≤ 1500 tokens. Prefer concise factual bullet points over prose.
- FORMAT: start with one line `[history summarized]`, then bullets. Do not fabricate facts not present in the source.
```

**Improvement opportunity:** The output token cap is `SUMMARIZE_OUTPUT_TOKEN_CAP = 800` (line 47), which can truncate the summary below the `≤ 1500 token` instruction in the prompt — an internal contradiction. The prompt was likely written before the cap was tightened.

---

## Surface 11 — Context overflow summarize user turn

**File:** `crates/xvision-engine/src/agent/summarize.rs:264`  
**Function:** `summarize_history()` (lines 236–295)  
**Path:** Legacy only  
**Operator-visible:** No

Exact template (lines 264–266):

```
Summarize the conversation history below per the system prompt rules. Source estimated_tokens={prefix_tokens} dropped_from_head={dropped_from_head}.

{rendered_prefix}
```

`rendered_prefix` is the output of `render_prefix_for_summarize()` — a turn-by-turn dump where tool inputs are clipped at 240 chars each.

**Improvement opportunity:** `estimated_tokens` uses a char/4 heuristic (line 52), not a real tokenizer. For reasoning models or unicode-heavy content the actual token count may differ substantially, causing the summarizer to receive an undersized or oversized budget. The drop count `dropped_from_head` in the prompt is informational only; the model cannot use it to reconstruct what was dropped.

---

## Surface 12 — V2D memory prior-observations system prompt prepend

**File:** `crates/xvision-engine/src/agent/memory_recorder.rs` (`render_recalled_patterns`)  
**Applied at:** `crates/xvision-engine/src/agent/execute.rs:381`  
**Path:** Legacy only; only when `memory_mode != Off` and recall returns hits  
**Operator-visible:** Partially — trace dock shows `memory_recall` events with text preview; full block not surfaced

Assembly (lines 381–384):

```rust
let assembled_system_prompt = match prior_block {
    Some(block) => format!("{block}\n\n{}", input.system_prompt),
    None => input.system_prompt.clone(),
};
```

The `block` is a `<prior_observations>` XML-tagged block wrapping recalled pattern previews. It is prepended BEFORE the operator's system prompt.

**Improvement opportunity:** Prepending before the operator's system prompt means the model sees recalled patterns before its role definition. Some models weight earlier content more heavily, which can cause recalled precedents to override explicit operator instructions. Appending after the system prompt would be safer semantically.

---

## Surface 13 — Repeated tool failure error (ToolResult injection)

**File:** `crates/xvision-engine/src/agent/execute.rs:1031`  
**Function:** `repeated_tool_failure_result()`  
**Path:** Legacy only (injected as `ContentBlock::ToolResult { is_error: Some(true), ... }`)  
**Operator-visible:** No

Exact text (lines 1031–1039):

```
repeated_tool_failure: tool '{tool_name}' with this exact input has failed 3 times in this slot execution. The input is blocked for the remainder of this run. Retry with a different input or choose a different tool.
```

**When triggered:** `RepeatedToolFailureTracker` records ≥ `MAX_TOOL_RETRIES_PER_PAIR` (= 3) failures for the same `(tool_name, sha256(input))` pair in a single slot execution.

**Improvement opportunity:** The message does not surface the original failure reason — only that the pair is blocked. A model trying to self-heal sees "try something different" without knowing why the tool was failing. Including the last error message (available from the prior tool result in the conversation history) would give the model better signal.

---

## Surface 14 — Asset mismatch tool error (ToolResult injection)

**File:** `crates/xvision-engine/src/agent/execute.rs:97`  
**Function:** `market_data_tool_asset_mismatch()`  
**Path:** Legacy only; applies to `ohlcv` and `indicator_panel` tool calls  
**Operator-visible:** No

Exact template (lines 97–100):

```
tool error: asset mismatch for {tool_name}: current decision asset is {decision_asset} but tool requested {requested_asset}. Use the current decision asset only; do not fetch cross-asset market data for this per-asset decision.
```

Asset comparison normalizes: strips USD suffix, uppercases, takes base currency (e.g. `BTC/USD` → `BTC`).

**Improvement opportunity:** The normalization is silent — the error message does not explain why `BTC/USD` is considered the same as `BTC` or that normalization is happening. An operator who sees this for a mismatch like `BTC/USD` vs `BTC-USD` may be confused. Adding `(normalized: {normalized_decision_asset} vs {normalized_requested_asset})` would clarify.

---

## Surface 15 — Budget misconfig hint (tracing only; NOT sent to model)

**File:** `crates/xvision-engine/src/agent/recovery.rs:444`  
**Function:** `classify_budget_misconfig()`  
**Path:** Cline path; the warn is emitted at `execute_cline.rs:468`  
**Operator-visible:** Via `tracing::warn!` only — NOT sent to the model, NOT in `eval_runs.error`

Text stored in `FailureClass::BudgetMisconfig.hint` (line 444–446):

```
Slot max_tokens is too low; reasoning models need ≥2048 output tokens to produce a first decision. Increase max_tokens in the agent slot settings.
```

**Improvement opportunity:** The hint is actionable but lives only in logs. The `eval_runs.error` column carries `[budget_misconfig]` without the human-readable explanation. Surfacing the hint in the dashboard's run-detail error field would allow operators to act without reading logs.

---

## Surface 16 — Agent slot starter template system prompts

**File:** `crates/xvision-engine/src/agents/templates.rs:69` (`builtin_templates()`)  
**Path:** Both Legacy and Cline — these become `AgentSlot.system_prompt` after the operator saves  
**Operator-visible:** Yes — shown in `/agents/new` template picker; editable before saving

Nine built-in templates provide 18 slot prompts total. Selected exact texts:

**`single-trader` / `main`** (line 81):
```
You are a discretionary trader making one decision per cycle. Given the briefing, output exactly one JSON object matching: {"action":"long_open|short_open|flat|hold", "conviction":0..1, "justification":"string"}. Do not omit action.
```

**`risk-checked-trader` / `risk_check`** (line 186):
```
You are a risk gate. Given the trader's proposed decision and the current portfolio state, output {verdict: approve|modify|veto, size_cap_pct, reason}.
```

**`regime-aware-trader` / `regime`** (line 410):
```
You are a regime classifier. Read the briefing and label the current market regime. Output exactly one JSON object matching: {"regime":"trending_up|trending_down|range_bound|high_vol|low_vol|risk_off", "confidence":0..1, "evidence":"string"}. Evidence should cite the specific indicators, breadth, or volatility readings that drove the label.
```

**`news-reader-plus-trader` / `news`** (line 472):
```
You are a news reader. Read any headlines, transcripts, or narrative context attached to the briefing and produce a structured digest. Output exactly one JSON object matching: {"top_themes":["string"], "sentiment":"risk_on|risk_off|mixed", "event_risks":["string"], "summary":"string"}. If no narrative input is present, return empty arrays and `"sentiment":"mixed"` with a summary noting the absence.
```

**`paper-confirmed-live-trader` / `executor`** (line 559):
```
You are a live executor. Read the paper trader's proposal and decide whether to confirm it for live commit, downgrade it (e.g. to `hold` or lower conviction), or veto it to `flat`. Be stricter than the paper trader: require the proposal's primary case to remain valid after considering its own listed risks. Output exactly one JSON object matching: {"action":"long_open|short_open|flat|hold", "conviction":0..1, "justification":"string"}. Justification must explicitly state confirm / downgrade / veto and why. Do not omit action.
```

**Improvement opportunity:**
- All trader-role slot prompts embed a prose schema (`{"action": ..., "conviction": ..., "justification": ...}`) that duplicates the formal `ResponseSchema::trader_output()`. When the formal schema gains a new field, the inline version becomes stale.
- The `risk_check` slot uses a pseudo-JSON syntax `{verdict: approve|modify|veto, size_cap_pct, reason}` with no types or validation — the least structured schema of any built-in template. A formal `ResponseSchema` for the Critic capability would reduce `MissingField` / `InvalidJson` failures from risk-gate slots.

---

## Surface 17 — `trader_output` JSON Schema (sent via structured-output constraint)

**File:** `crates/xvision-engine/src/agent/llm.rs` (`ResponseSchema::trader_output()`)  
**Path:** Both — sent as `LlmRequest.response_schema` (Legacy) or `StartRunParams.decision_schema` (Cline)  
**Operator-visible:** No (applied automatically for slots whose role is `trader`)

Exact schema:

```json
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "action": {
      "type": "string",
      "enum": ["long_open", "short_open", "flat", "hold"]
    },
    "conviction": {
      "type": "number",
      "minimum": 0.0,
      "maximum": 1.0
    },
    "justification": {
      "type": "string",
      "minLength": 1
    }
  },
  "required": ["action", "conviction", "justification"]
}
```

How it is applied:
- **Anthropic (Legacy):** Injected as a system-prompt suffix block `"You must respond with exactly one JSON object matching this JSON Schema…"`.
- **OpenAI-compat (Legacy):** Sent as `response_format: {type: "json_schema", json_schema: {...}}`.
- **Cline sidecar:** Passed as `StartRunParams.decision_schema`; the sidecar's `buildSubmitDecisionTool()` validates the `submit_decision` argument against it at call time.

**Improvement opportunity:** Only `trader`-role slots receive this schema automatically (`response_schema_for_slot()` at `execute.rs:1041`). Non-trader slots (critic, filter, router) rely entirely on their prose system prompts to describe output shape and have no structured validation guard. Adding `ResponseSchema` support per-capability would reduce failures on non-trader slots and enable repair paths (Surfaces 8 and 9) for those capabilities.

---

## Cross-surface conflict map

| Surfaces | Scenario | Live risk |
|---|---|---|
| S7 (`sanitizeSystemPrompt`) + any operator prompt containing "output json" | Sidecar appends "Ignore earlier instructions to output raw JSON" which can cause `NoDecision` | Yes — monitor `tracing::info event=nodecision_recovery_succeeded` to catch |
| S1 ("Output JSON only") + S7 (sidecar regex) | Stage 1 preamble would trigger the sidecar correction note if Stage 1 ever routed through Cline | Currently unreachable; becomes live if routing changes |
| S12 (memory prepend) + operator system prompt | Recalled patterns appear before role definition; may override explicit instructions on some models | Low; mitigated by 5-item recall cap |
| S8 / S9 repair paths + Cline path | Repair messages say "do not include tool calls" but Cline requires `submit_decision` | Currently unreachable — repair paths are Legacy-only |
| S4 (Legacy: "emit your final decision as JSON") + S17 (structured schema) | Two format instructions; schema is authoritative | Benign on trader slots; harmful on non-trader slots with no schema guard |

---

## Summary table

| # | Surface | File:line | Type | Stage1 | Legacy | Cline | Op-visible |
|---|---|---|---|---|---|---|---|
| 1 | Intern system preamble | `prompt.rs:173` | System prompt | ✓ | — | — | No |
| 2 | Intern schema instructions | `prompt.rs:183` | User msg suffix | ✓ | — | — | No |
| 3 | Intern backend system prompt | `backend.rs:214/292` | System prompt | ✓ | — | — | No |
| 4 | Legacy initial user turn | `execute.rs:260` | User msg | — | ✓ | — | No |
| 5 | Cline initial user turn | `execute_cline.rs:327` | User msg | — | — | ✓ | No |
| 6 | Cline no-decision recovery | `execute_cline.rs:561` | User msg (repair) | — | — | ✓ | No |
| 7 | Sidecar prompt sanitizer | `build-agent.ts:4` | System mutation | — | — | ✓ | No |
| 8 | MalformedJson repair msg | `recovery.rs:547` | User msg (repair) | — | ✓ | — | No |
| 9 | SchemaMissingField repair msg | `recovery.rs:859` | User msg (repair) | — | ✓ | — | No |
| 10 | Summarize system prompt | `summarize.rs:80` | System prompt | — | ✓ | — | No |
| 11 | Summarize user turn | `summarize.rs:264` | User msg | — | ✓ | — | No |
| 12 | Memory prior-observations | `execute.rs:381` | System prefix | — | ✓ | — | Partial |
| 13 | Repeated-tool-failure error | `execute.rs:1031` | ToolResult injection | — | ✓ | — | No |
| 14 | Asset-mismatch error | `execute.rs:97` | ToolResult injection | — | ✓ | — | No |
| 15 | Budget misconfig hint | `recovery.rs:444` | Tracing only | — | — | ✓ | No |
| 16 | Template starter prompts | `templates.rs:69` | System prompt seed | — | ✓ | ✓ | Yes |
| 17 | `trader_output` JSON Schema | `llm.rs` | Schema constraint | — | ✓ | ✓ | No |
