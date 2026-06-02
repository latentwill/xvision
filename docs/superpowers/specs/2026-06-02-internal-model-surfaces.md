# Internal Model Surfaces — Reference Catalog

**Date:** 2026-06-02  
**Scope:** All strings/templates the harness sends to LLMs as system prompts, user messages, or tool descriptions — invisible to operators in the dashboard UI.

Operators debugging model failures (wrong JSON shape, ignored instructions, tool-call refusals) should consult this catalog to understand what the model actually sees beyond the operator-authored system prompt.

---

## Surface 1 — Legacy initial user turn

**File:** `crates/xvision-engine/src/agent/execute.rs:260-265`  
**Type:** User message (first turn)  
**Models:** All models on the legacy (non-Cline) execution path  
**Operator-visible:** No

```
Inputs:
{upstream_inputs_pretty_json}

Follow the slot's instructions. You may call tools to fetch additional data for the current decision asset only; emit your final decision as JSON.
```

**Improvement opportunity:** "emit your final decision as JSON" is ambiguous when the slot's system prompt also ends with a `ResponseSchema::prompt_contract` block (Surface 5) that says "respond with exactly one JSON object". The two instructions are consistent but redundant; a model that missed the system-prompt contract might treat the user-turn wording as the authoritative format spec and omit required fields that exist only in the schema. The Cline path (Surface 2) strengthens this to a hard `submit_decision` requirement.

---

## Surface 2 — Cline initial user turn

**File:** `crates/xvision-engine/src/agent/execute_cline.rs:327-338` (`ClineSlotInput::render_prompt`)  
**Type:** User message (first turn)  
**Models:** All models on the Cline execution path  
**Operator-visible:** No

```
Inputs:
{upstream_inputs_pretty_json}

Follow the slot instructions. You may call tools to fetch additional data for the current decision asset only. You MUST reply by calling the submit_decision tool with your decision as a JSON argument. Do NOT output prose, raw JSON, or code fences in your reply — only the submit_decision tool call is accepted. Outputting text instead of calling the tool will fail the cycle.
```

**Improvement opportunity:** None critical. The instruction is explicit. However, models that received a system prompt containing "output JSON only" (common in operator-authored prompts) may conflict with this directive — the `sanitizeSystemPrompt` sidecar function (Surface 8) addresses exactly this but only runs on the Cline path.

---

## Surface 3 — MalformedJson repair message

**File:** `crates/xvision-engine/src/agent/recovery.rs:547-566` (`build_malformed_json_repair_message`)  
**Type:** User message (recovery turn — second attempt only)  
**Models:** Legacy and Cline paths; triggered when first response is `InvalidJson` or `Truncated`  
**Operator-visible:** No (visible in trace dock spans tagged `recovery.attempt`)

```
Your previous response failed to parse: {parse_error}

Emit a single JSON object matching the `{schema_name}` schema below. Do not include prose, code fences, or tool calls. Return ONLY the JSON object.

Schema:
{schema_body_pretty_json}
```

**Improvement opportunity:** "Do not include … tool calls" conflicts with the Cline path, where the model is required to use `submit_decision`. On the legacy path this instruction is correct; on the Cline path a malformed-JSON repair would never be triggered (the sidecar captures decisions before the Rust parser sees them), so the conflict is currently unreachable. If the repair path is ever extended to Cline, this instruction must be updated to allow the `submit_decision` tool call.

---

## Surface 4 — SchemaMissingField repair message

**File:** `crates/xvision-engine/src/agent/recovery.rs:859-878` (`build_schema_missing_field_repair_message`)  
**Type:** User message (recovery turn — second attempt only)  
**Models:** Legacy path; triggered when first response is `MissingField` or `InvalidField`  
**Operator-visible:** No (visible in trace dock spans tagged `recovery.attempt`)

```
Your previous response was missing or invalid for the following fields: [{fields}].

Re-emit ONLY a single JSON object containing those fields, filled in correctly. The other fields you produced are accepted as-is — do not repeat them. Do not include prose, code fences, or tool calls.

Validator detail: {parse_error}
```

**Improvement opportunity:** "Re-emit ONLY a single JSON object containing those fields" is a partial-patch instruction. Models that don't understand selective patching may repeat all fields — which the merge logic handles correctly (`merge_and_reparse_trader_output`), but the extra repetition wastes tokens. Consider clarifying: "emit a JSON object with ONLY the listed fields; other fields will be merged from your prior response."

---

## Surface 5 — ResponseSchema system-prompt contract

**File:** `crates/xvision-engine/src/agent/llm.rs:443-449` (`ResponseSchema::prompt_contract`)  
**Applied at:** `crates/xvision-engine/src/agent/llm.rs:700-701` (`anthropic_request_body`)  
**Type:** System prompt suffix (appended to operator system prompt)  
**Models:** All models dispatched via `anthropic_request_body` — Trader, Router; NOT Filter (which uses Surface 6 instead)  
**Operator-visible:** No

```
\n\nYou must respond with exactly one JSON object matching this JSON Schema. Do not include markdown, prose, or extra keys.
Schema `{schema_name}`:
{schema_json}
```

**Improvement opportunity:** This suffix is always appended on the Anthropic provider path even for Cline-path runs that also receive a `submit_decision` tool. The tool description (Surface 7) and this suffix both instruct the model on output format but via different mechanisms (tool call vs raw JSON). `sanitizeSystemPrompt` (Surface 8) partially mitigates conflicts when the operator prompt contains "output JSON only", but the schema suffix itself is not suppressed on the Cline path, creating a latent conflict for models that treat the schema suffix as the authoritative instruction and emit raw JSON instead of calling the tool.

---

## Surface 6 — Filter capability system-prompt contract

**File:** `crates/xvision-engine/src/agent/filter_dispatch.rs:149-156` (`filter_prompt_contract`)  
**Applied at:** `crates/xvision-engine/src/agent/filter_dispatch.rs:74-77`  
**Type:** System prompt (replaces or appends to operator system prompt)  
**Models:** Models in slots with `Capability::Filter`; legacy path only  
**Operator-visible:** No

```
You are a Filter. Emit exactly one JSON object matching the response schema: {"name": <string>, "payload": <object>, "granularity": "bar" | "minute" | "decision"}. No markdown, no prose. The `payload` is the structured signal downstream agents read via edge predicates.
```

When the operator's `system_prompt` is non-empty, this string is appended after `\n\n`. When the operator's system prompt is empty, this string IS the full system prompt.

**Improvement opportunity:** The inline schema description is a prose approximation, not actual JSON Schema. If the operator's slot prompt also describes an output format, both compete for the model's attention. The filter schema (`filter_response_schema` at `filter_dispatch.rs:127`) is never sent as a formal JSON Schema constraint block (unlike Trader which uses `ResponseSchema::prompt_contract`). Making Filter use `ResponseSchema` + the formal schema block would unify the pattern and make failures more debuggable.

---

## Surface 7 — `submit_decision` tool description

**File:** `xvision-agentd/src/session/submit-decision.ts:31-35` (`buildSubmitDecisionTool`)  
**Type:** Tool description (part of the model's tool list)  
**Models:** All Cline-path models  
**Operator-visible:** No

```
Submit your final structured decision as a single JSON object matching the provided schema. Call this exactly once; the call completes the run.
```

The tool also carries `inputSchema` (the slot's `decision_schema` JSON Schema), so the model sees the formal schema constraint — not just the prose description.

**Improvement opportunity:** "Call this exactly once" is not enforced structurally (the `lifecycle.completesRun: true` flag terminates the run on first call, but a model confused about ordering might call another tool after `submit_decision` and expect a response). The description could add: "No further tool calls will be processed after this one."

---

## Surface 8 — Sidecar system-prompt sanitizer

**File:** `xvision-agentd/src/session/build-agent.ts:4-15` (`sanitizeSystemPrompt`)  
**Applied at:** `build-agent.ts:51`  
**Type:** System prompt mutation (appends correction note when conflict detected)  
**Models:** All Cline-path models  
**Operator-visible:** No

Regex trigger (case-insensitive):
```
/output[^\n]*json[^\n]*only|strict\s+json|json\s+only|output\s+json/i
```

Correction text appended when triggered:
```
\n\nIMPORTANT: Ignore any earlier instructions to output raw JSON. You MUST call the submit_decision tool to submit your decision — outputting JSON text is not accepted and will fail the cycle.
```

The function is idempotent — if the correction marker (`"You MUST call the submit_decision tool"`) is already present, it skips appending.

**Improvement opportunity:** The regex covers the most common patterns but misses variants like "respond with only JSON", "return pure JSON", "your response should be JSON", or non-English prompts. Operators who write system prompts like "Respond only with JSON, no commentary" will NOT trigger the sanitizer and may encounter silent Cline failures. A broader heuristic or explicit operator documentation of this surface would reduce surprise.

---

## Surface 9 — Memory recall `<prior_observations>` block

**File:** `crates/xvision-engine/src/agent/memory_recorder.rs:174-192` (`render_recalled_patterns`)  
**Applied at:** `crates/xvision-engine/src/agent/execute.rs:319` (prepended to system prompt)  
**Type:** System prompt prefix (prepended when memory hits exist)  
**Models:** Legacy path only; only when `memory_mode != Off` and embedder is active  
**Operator-visible:** Partially — the trace dock emits `memory_recall` events with a `text_preview` of each match, but the full block sent to the model is not surfaced

Template (one entry per memory match, bounded by the slot's recall limit):
```
<prior_observations>
A prior decision noted: "{pattern_text_preview_160_chars}". Consider whether this situation matches the present cycle.
[... repeated per match ...]
</prior_observations>
```

**Improvement opportunity:** The framing "Consider whether this situation matches" is deliberately non-directive (it's a precedent hint, not an instruction). However, if the operator's system prompt also includes instructions about how to use historical context, the two framings may conflict or reinforce each other unpredictably. The `<prior_observations>` tag is stable (referenced in Phase 5 MemoryPanel UI), but the inner wording is not versioned — changes here affect active strategies silently.

---

## Surface 10 — Agent starter-template system prompts

**File:** `crates/xvision-engine/src/agents/templates.rs:69+`  
**Type:** Pre-filled operator system prompts (shown in the UI form, editable before saving)  
**Models:** Whatever model the operator selects; applies after operator customization  
**Operator-visible:** Yes (editable in the agents form)

These are user-visible defaults, not injected text. Documented here because they contain inline schema specifications that can conflict with `ResponseSchema::prompt_contract` (Surface 5) if operators save them without modification.

Selected examples:

- **`single-trader` / `main` slot** (`templates.rs:81-84`):
  ```
  You are a discretionary trader making one decision per cycle. Given the briefing, output exactly one JSON object matching: {"action":"long_open|short_open|flat|hold", "conviction":0..1, "justification":"string"}. Do not omit action.
  ```

- **`analyst-executor` / `analyst` slot** (`templates.rs:111-113`):
  ```
  You are a market analyst. Read the briefing and output a structured thesis: regime, dominant signal, contradicting signals, expected volatility, time horizon.
  ```

- **`analyst-executor` / `executor` slot** (`templates.rs:135-138`):
  ```
  You are an executor. Given the analyst's thesis, output a single JSON decision matching: {"action":"long_open|short_open|flat|hold", "conviction":0..1, "justification":"string"}. Be conservative when the analyst flags contradictions. Do not omit action.
  ```

**Improvement opportunity:** The trader-role slot prompts embed a prose schema that duplicates the formal `ResponseSchema::trader_output()` JSON Schema appended at dispatch time (Surface 5). If a new field is added to the formal schema, the inline version becomes stale and models may omit the new field. The template prompts should either remove the inline schema (trusting Surface 5) or reference it symbolically.

---

## Surface 11 — Broker error feedback synthetic turn

**File:** `crates/xvision-engine/src/agent/execute.rs:278-305`  
**Type:** Synthetic prior-turn pair (assistant ToolUse + user ToolResult with `is_error: true`) injected before the live user turn  
**Models:** Legacy path only; only when the previous cycle had a recoverable broker error  
**Operator-visible:** No

The synthetic ToolUse carries:
```json
{
  "id": "broker_call_prior_cycle_{decision_index}",
  "name": "broker.submit_order",
  "input": {"asset": "<asset>", "intended_action": "broker submit"}
}
```

Followed by a ToolResult with `is_error: true` and the broker error body.

**Improvement opportunity:** `broker.submit_order` is not in the slot's `allowed_tools` for the current turn. Some models may interpret the ToolResult as a permission signal that `broker.submit_order` is available and attempt to call it, triggering a tool-not-found error. Adding a brief clarifying note to the error result — "this error is from the prior cycle; do not attempt to call broker.submit_order this cycle" — would reduce confusion.

---

## Surface 12 — NoDecision recovery retry prompt

**File:** `crates/xvision-engine/src/agent/execute_cline.rs` (recovery path; see commits `954a101`, `ae1316c`)  
**Type:** User message (recovery turn — injected when Cline run completes without calling `submit_decision`)  
**Models:** Cline-path models only  
**Operator-visible:** No

Triggered by the `NoDecision` recovery path introduced in wave2 harness improvements. The recovery retries the step with a reminder prompt before failing the cycle.

**Improvement opportunity:** The retry prompt should echo the original `render_prompt` user turn content (Surface 2) so the model has full context on the second attempt rather than only the recovery directive. Without the original inputs, a model may call `submit_decision` with an empty or default decision.

---

## Summary Table

| # | Surface | File | Type | Cline | Legacy | Op-visible |
|---|---|---|---|---|---|---|
| 1 | Initial user turn | `execute.rs:260` | User msg | No | Yes | No |
| 2 | Cline initial user turn | `execute_cline.rs:327` | User msg | Yes | No | No |
| 3 | MalformedJson repair | `recovery.rs:547` | User msg | No | Yes | No |
| 4 | SchemaMissingField repair | `recovery.rs:859` | User msg | No | Yes | No |
| 5 | ResponseSchema contract | `llm.rs:443` | Sys suffix | Partial | Yes | No |
| 6 | Filter capability contract | `filter_dispatch.rs:149` | Sys prompt | No | Yes | No |
| 7 | `submit_decision` tool desc | `submit-decision.ts:31` | Tool desc | Yes | No | No |
| 8 | Sidecar sanitizer | `build-agent.ts:4` | Sys mutation | Yes | No | No |
| 9 | Memory `<prior_observations>` | `memory_recorder.rs:174` | Sys prefix | No | Yes | Partial |
| 10 | Template starter prompts | `templates.rs:69` | Sys prompt | Both | Both | Yes |
| 11 | Broker error synthetic turn | `execute.rs:278` | Synthetic turn | No | Yes | No |
| 12 | NoDecision recovery prompt | `execute_cline.rs` (recovery) | User msg | Yes | No | No |
