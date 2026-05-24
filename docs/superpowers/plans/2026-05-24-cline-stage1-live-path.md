# Cline Runtime Unification — Stage 1: Cline Live Path (the deferred Wave 3) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route live + forward-paper decision cycles through the `xvision-agentd` Cline sidecar so a real cycle produces a `TraderDecision` end-to-end via Cline, with `LlmDispatch` demoted to a flag-gated fallback.

**Architecture:** A slot invocation becomes a *Cline `Agent` run* (`start_run` → one-or-more `step` → `end_run`), per the umbrella. We do **not** wrap Cline inside `LlmDispatch` — that would nest `execute_slot`'s tool loop inside Cline's own loop. Instead we add a sibling executor `execute_slot_cline` that returns the same `LlmResponse` shape (so `PipelineOutputs` and all downstream parsing are unchanged), and a runtime flag selects it. The agent returns its structured decision by calling a new `submit_decision` lifecycle tool; `xvision-mcp` indicators are registered as Cline tools. Observability flows through the existing event sink so live runs appear in the agent-runs UI.

**Tech Stack:** Rust (`xvision-engine`, `xvision-agent-client`, `xvision-core`), TypeScript/Vitest (`xvision-agentd`), the `@cline/sdk` `Agent`, React (`frontend/web`).

**Umbrella spec:** `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md` (Stage 1 section + "Subplan inheritance contract").

---

## Inherited contract gates (from umbrella §"Subplan inheritance contract")

All boxes must be checked before this stage is accepted.

- [ ] **Item 2 — Failure + recovery (non-negotiable).** Define and test: (a) sidecar crash boundary during an in-flight `step` surfaces as a typed error and marks the cycle failed (never silently dropped); (b) `run_id` is the dedup/idempotency key so a retried `start_run` does not double-execute; (c) partial-cycle recovery semantics (a crashed slot fails its cycle cleanly, upstream slot outputs are preserved). Live-vs-replay divergence is **n/a at Stage 1** (no replay yet) — recorded as a Stage 3 obligation here so it is not lost.
- [ ] **Item 3 — Operational visibility (non-negotiable, begins here).** Live runs surface run/slot phase state and a `trajectory_mode` field (value `"live"` at this stage) in: the CLI (`xvn run inspect`), the agent-runs dashboard (re-enable the disabled mode badge), and structured run artifacts. Replay-hit ratio / dropped-events / recovery-reason fields are *declared* now and populated in Stages 2–3.
- [ ] **Item 5 — Provider matrix + compatibility (must-have).** Produce an explicit provider coverage matrix (xvision `ProviderEntry` → Cline `providerId`/`modelId`/`baseUrl`, feature parity) committed as a doc, and define fallback/abort behavior per gap: an unmapped provider **aborts with a typed error**, it does not silently fall back unless the runtime flag explicitly selects `LlmDispatch`.

Stage 1 exit (umbrella): *a real live/forward-paper cycle produces a `TraderDecision` via the Cline sidecar end-to-end; `LlmDispatch` no longer the live path (flag-gated fallback only).*

---

## File Structure

- Create: `crates/xvision-agent-client/src/provider_map.rs` — `ProviderEntry` (kind + base_url) → Cline `providerId`/`modelId` mapping + typed `ProviderMapError`.
- Create: `docs/superpowers/specs/2026-05-24-cline-provider-matrix.md` — the item-5 coverage matrix deliverable.
- Modify: `crates/xvision-core/src/config.rs` — add `AgentRuntime` enum + `runtime` field.
- Create: `crates/xvision-engine/src/agent/execute_cline.rs` — `execute_slot_cline(SlotInput) -> LlmResponse`.
- Modify: `crates/xvision-engine/src/agent/execute.rs` — re-export + shared `SlotInput`.
- Modify: `crates/xvision-engine/src/agent/pipeline.rs` — runtime-flag branch in `run_agent_pipeline`/`run_pipeline`.
- Modify: `crates/xvision-engine/src/api/eval.rs` — construct an `AgentClient` (event-sink-wired) when `runtime == Cline`.
- Modify: `crates/xvision-agent-client/src/protocol.rs` — add `decision_json: Option<String>` to `StepResult`.
- Modify (Node): `xvision-agentd/src/session/submit-decision.ts` (new), `src/methods/session.ts`, `src/session/store.ts`, `src/tool-registry.ts`.
- Modify: `crates/xvision-engine/migrations/0NN_run_trajectory_mode.sql` — add `trajectory_mode` to `agent_runs`.
- Modify: `frontend/web/src/routes/agent-runs-detail.tsx:156` + `frontend/web/src/features/agent-runs/RunStatusStrip.tsx` + `frontend/web/src/api/types-agent-runs.ts`.
- Modify: `crates/xvision-cli/src/commands/run/inspect.rs` — print `trajectory_mode`.

---

### Task 1: Provider mapping + coverage matrix (item 5)

**Files:**
- Create: `crates/xvision-agent-client/src/provider_map.rs`
- Modify: `crates/xvision-agent-client/src/lib.rs` (add `pub mod provider_map;`)
- Create: `docs/superpowers/specs/2026-05-24-cline-provider-matrix.md`
- Test: inline `#[cfg(test)] mod tests` in `provider_map.rs`

> **Audit prerequisite (not a placeholder — a real first step):** The Cline SDK's `providerId` vocabulary is not present in this repo (verified). Before writing the table values, read `node_modules/@cline/sdk` type defs (or upstream docs) and record the exact accepted `providerId` strings (e.g. `anthropic`, `openai`, `openrouter`, …). Fill the `CLINE_PROVIDER_ID` constants below with the audited strings. The *structure* is fixed; the string values come from the audit and are recorded in the matrix doc.

- [ ] **Step 1: Write the failing mapping test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::config::{ProviderEntry, ProviderKind};

    fn entry(kind: ProviderKind, base_url: &str) -> ProviderEntry {
        ProviderEntry {
            name: "p".into(), kind, base_url: base_url.into(),
            api_key_env: "K".into(), enabled_models: vec!["m".into()],
        }
    }

    #[test]
    fn anthropic_maps_to_cline_anthropic() {
        let m = map_provider(&entry(ProviderKind::Anthropic, ""), "claude-opus-4-7").unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_ANTHROPIC);
        assert_eq!(m.model_id, "claude-opus-4-7");
        assert_eq!(m.base_url, None);
    }

    #[test]
    fn openai_compat_passes_base_url_through() {
        let m = map_provider(&entry(ProviderKind::OpenaiCompat, "https://openrouter.ai/api/v1"), "x").unwrap();
        assert_eq!(m.provider_id, CLINE_PROVIDER_OPENAI_COMPAT);
        assert_eq!(m.base_url.as_deref(), Some("https://openrouter.ai/api/v1"));
    }

    #[test]
    fn local_candle_is_unmappable_and_aborts() {
        let err = map_provider(&entry(ProviderKind::LocalCandle, ""), "x").unwrap_err();
        assert!(matches!(err, ProviderMapError::Unsupported { .. }));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p xvision-agent-client provider_map`
Expected: FAIL — `map_provider` / `ProviderMapError` not defined.
(Per the no-cargo-in-main-checkout rule: run this from a worktree with `CARGO_TARGET_DIR=$HOME/.cargo-target/xvision-stage1`, not the shared main checkout.)

- [ ] **Step 3: Implement the mapping**

```rust
use xvision_core::config::{ProviderEntry, ProviderKind};

// Audited from @cline/sdk — fill with the exact strings the SDK accepts.
pub const CLINE_PROVIDER_ANTHROPIC: &str = "anthropic";
pub const CLINE_PROVIDER_OPENAI_COMPAT: &str = "openai"; // OpenAI-compatible gateway

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClineProvider {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderMapError {
    #[error("provider kind {kind:?} has no Cline mapping (provider '{name}')")]
    Unsupported { name: String, kind: ProviderKind },
}

pub fn map_provider(entry: &ProviderEntry, model_id: &str) -> Result<ClineProvider, ProviderMapError> {
    let (provider_id, base_url) = match entry.kind {
        ProviderKind::Anthropic => (CLINE_PROVIDER_ANTHROPIC.to_string(), None),
        ProviderKind::OpenaiCompat => (
            CLINE_PROVIDER_OPENAI_COMPAT.to_string(),
            Some(entry.base_url.clone()).filter(|s| !s.is_empty()),
        ),
        ProviderKind::LocalCandle => {
            return Err(ProviderMapError::Unsupported {
                name: entry.name.clone(),
                kind: entry.kind,
            })
        }
    };
    Ok(ClineProvider { provider_id, model_id: model_id.to_string(), base_url })
}
```
(`ProviderKind` must derive `Debug` — confirm in `config.rs`; add if missing in a one-line edit.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p xvision-agent-client provider_map`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the coverage-matrix doc deliverable**

Create `docs/superpowers/specs/2026-05-24-cline-provider-matrix.md` with a table: one row per xvision-supported provider (Anthropic; OpenAI-compat family: OpenRouter, DeepSeek, Groq, Together, Mistral, xAI, Fireworks, Perplexity, custom `baseUrl`), columns: xvision `ProviderKind`, declared `base_url`, mapped Cline `providerId`, feature-parity notes (tool calling? streaming? JSON-schema response?), and **gap → behavior** (mapped / abort). Note explicitly that `LocalCandle` (mock) aborts under the Cline runtime and stays on `LlmDispatch`.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-agent-client/src/provider_map.rs crates/xvision-agent-client/src/lib.rs \
        docs/superpowers/specs/2026-05-24-cline-provider-matrix.md
git commit -m "feat(stage1): provider→Cline mapping + coverage matrix"
```

---

### Task 2: Runtime selection flag

**Files:**
- Modify: `crates/xvision-core/src/config.rs`
- Test: inline `#[cfg(test)]` in `config.rs`

- [ ] **Step 1: Write the failing config test**

```rust
#[test]
fn agent_runtime_defaults_to_llm_dispatch_until_flipped() {
    // Default stays LlmDispatch during Stage 1 build-out; Task 10 flips it.
    assert_eq!(AgentRuntime::default(), AgentRuntime::LlmDispatch);
}

#[test]
fn agent_runtime_parses_from_str() {
    assert_eq!("cline".parse::<AgentRuntime>().unwrap(), AgentRuntime::Cline);
    assert_eq!("llm-dispatch".parse::<AgentRuntime>().unwrap(), AgentRuntime::LlmDispatch);
}
```

- [ ] **Step 2: Run it — FAIL** (`AgentRuntime` undefined). `cargo test -p xvision-core agent_runtime` (from worktree target dir).

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRuntime {
    #[default]
    LlmDispatch,
    Cline,
}

impl std::str::FromStr for AgentRuntime {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "cline" => Ok(Self::Cline),
            "llm-dispatch" | "llm_dispatch" => Ok(Self::LlmDispatch),
            other => Err(format!("unknown agent runtime: {other}")),
        }
    }
}
```
Add a `pub runtime: AgentRuntime` field (with `#[serde(default)]`) to the engine/runtime config struct that flows into `PipelineInputs` construction (the same config object `api/eval.rs` reads provider config from).

- [ ] **Step 4: Run — PASS.** Step 5: Commit `feat(stage1): add AgentRuntime selection flag (default LlmDispatch)`.

---

### Task 3: `submit_decision` lifecycle tool (sidecar / Node)

**Files:**
- Create: `xvision-agentd/src/session/submit-decision.ts`
- Modify: `xvision-agentd/src/methods/session.ts` (capture decision; add to result)
- Modify: `xvision-agentd/src/session/store.ts` (`StepResult` shape; per-run `decisionJson`)
- Test: `xvision-agentd/test/session/submit-decision.test.ts`

- [ ] **Step 1: Write the failing vitest**

```typescript
import { describe, it, expect, beforeEach } from "vitest"
import { handleSessionStartRun, handleSessionStep, __setStoreForTesting } from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { setMockScript, resetMockScript } from "../../src/testing/mock-provider.js"
import { resetRegistry } from "../../src/tool-registry.js"

describe("submit_decision lifecycle tool", () => {
  beforeEach(() => {
    resetMockScript(); resetRegistry()
    __setStoreForTesting(createStore({ now: () => 0 }))
  })

  it("captures the decision payload from the submit_decision tool call", async () => {
    // Mock agent: one turn that calls submit_decision, then finishes.
    setMockScript([{ toolCall: { name: "submit_decision", input: { action: "buy", size: 1 } } }])
    handleSessionStartRun({
      run_id: "r1", provider_id: "xvision-mock", model_id: "mock",
      system_prompt: "decide", allowed_tools: ["submit_decision"],
      budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 10000 },
    })
    const r = await handleSessionStep({ run_id: "r1", prompt: "go" })
    expect(r.status).toBe("completed")
    expect(JSON.parse(r.decision_json!)).toEqual({ action: "buy", size: 1 })
  })
})
```

- [ ] **Step 2: Run — FAIL.** `cd xvision-agentd && npm run test -- submit-decision` (no `decision_json`, tool not registered).

- [ ] **Step 3: Implement `submit-decision.ts`**

```typescript
// A lifecycle tool the agent calls exactly once to emit its structured
// decision. The sidecar captures the input and ends the run.
import type { AgentTool } from "../tool-registry.js"

export const SUBMIT_DECISION_TOOL = "submit_decision"

export function buildSubmitDecisionTool(capture: (json: string) => void): AgentTool {
  return {
    name: SUBMIT_DECISION_TOOL,
    description: "Submit your final structured decision. Call exactly once. Ends the run.",
    inputSchema: { type: "object", additionalProperties: true }, // slot-specific schema injected by caller
    isRunTerminator: true,
    async run(input: unknown) {
      capture(JSON.stringify(input))
      return { ok: true }
    },
  }
}
```

In `session.ts`: when building the agent for a run whose `allowed_tools` includes `submit_decision`, register `buildSubmitDecisionTool((json) => store.setDecisionJson(run_id, json))` alongside the shimmed tools. In `store.ts`: add `decisionJson?: string` to the per-run session and `setDecisionJson`/`getDecisionJson`. Extend `StepResult` with `decision_json?: string`, populated from `store.getDecisionJson(run_id)` after `agent.run`.

- [ ] **Step 4: Run — PASS.** Step 5: Commit `feat(stage1): submit_decision lifecycle tool in sidecar`.

---

### Task 4: Rust protocol — `decision_json` + tool registration

**Files:**
- Modify: `crates/xvision-agent-client/src/protocol.rs` (add `decision_json: Option<String>` to `StepResult`)
- Modify: `crates/xvision-agent-client/src/client.rs` (helper to register `submit_decision` + indicator tools)
- Test: inline test in `xvision-agent-client`

- [ ] **Step 1: Failing test** — assert `StepResult` deserializes a payload containing `decision_json`:

```rust
#[test]
fn step_result_carries_decision_json() {
    let v = serde_json::json!({
        "status": "completed", "output_text": "", "iterations": 1,
        "usage": {"input_tokens":1,"output_tokens":1,"cache_read_tokens":0,"cache_write_tokens":0,"total_cost":0.0},
        "error": null, "decision_json": "{\"action\":\"buy\"}"
    });
    let r: StepResult = serde_json::from_value(v).unwrap();
    assert_eq!(r.decision_json.as_deref(), Some("{\"action\":\"buy\"}"));
}
```

- [ ] **Step 2: Run — FAIL** (unknown field / missing field). `cargo test -p xvision-agent-client step_result` (worktree target dir).

- [ ] **Step 3: Implement** — add `#[serde(default)] pub decision_json: Option<String>` to `StepResult`. Add a `register_decision_and_indicator_tools(&self, schema: serde_json::Value, indicators: Vec<ToolDescriptor>)` helper on `AgentClient` that pushes the `submit_decision` descriptor (with the slot's response schema as `inputSchema`) plus the `xvision-mcp` indicator descriptors over the existing `tool.registry.set` RPC.

- [ ] **Step 4: Run — PASS.** Step 5: Commit `feat(stage1): decision_json in StepResult + tool registration helper`.

---

### Task 5: `execute_slot_cline` — Cline-backed slot executor

**Files:**
- Create: `crates/xvision-engine/src/agent/execute_cline.rs`
- Modify: `crates/xvision-engine/src/agent/mod.rs` (`mod execute_cline; pub use …`)
- Test: `crates/xvision-engine/tests/cline_execute_slot.rs`

- [ ] **Step 1: Write the failing integration test**

Drives a slot through a sidecar running the mock provider; the mock script calls `submit_decision`. Assert the returned `LlmResponse` carries the decision as a `ToolUse`/structured block compatible with the existing parser.

```rust
mod common;
use xvision_agent_client::AgentClient;
// spawn the built sidecar (test helper resolves the agentd entrypoint + a temp UDS),
// set the mock script via a test RPC, run one slot, assert the decision round-trips.
#[tokio::test]
async fn cline_slot_returns_submit_decision_as_llm_response() {
    let client = common::spawn_mock_sidecar().await;
    let resp = xvision_engine::agent::execute_slot_cline(common::mock_slot_input(&client, r#"{"action":"buy","size":1}"#)).await.unwrap();
    let decision = common::extract_decision_json(&resp);
    assert_eq!(decision["action"], "buy");
}
```

- [ ] **Step 2: Run — FAIL** (`execute_slot_cline` undefined; `spawn_mock_sidecar` helper TBD in `tests/common`). Add the helper as part of this task (it shells the sidecar via the same path `AgentClient::spawn` uses, pointed at `xvision-agentd` build output, with `provider_id = "xvision-mock"`).

- [ ] **Step 3: Implement `execute_slot_cline`**

```rust
use crate::agent::execute::SlotInput;
use crate::agent::llm::{ContentBlock, LlmResponse, StopReason};
use xvision_agent_client::{provider_map::map_provider, AgentClient, protocol::*};

pub async fn execute_slot_cline<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    let client: &AgentClient = input.cline_client
        .ok_or_else(|| anyhow::anyhow!("Cline runtime selected but no AgentClient available"))?;
    let mapped = map_provider(input.provider_entry, input.slot.effective_model())?; // typed abort on gap (item 5)

    let run_id = input.run_id.to_string(); // cycle_id + slot role; the idempotency key (item 2)
    client.register_decision_and_indicator_tools(
        input.response_schema_json(), input.indicator_descriptors()).await?;
    client.start_run(StartRunParams {
        run_id: run_id.clone(),
        provider_id: mapped.provider_id, model_id: mapped.model_id,
        api_key: Some(input.api_key.clone()), base_url: mapped.base_url,
        system_prompt: input.system_prompt.clone(),
        allowed_tools: input.allowed_tool_names_plus_submit_decision(),
        budget_limits: input.budget_limits(),
    }).await?;

    let step = client.step(StepParams { run_id: run_id.clone(), prompt: input.render_prompt() }).await;
    let _ = client.end_run(EndRunParams { run_id }).await; // always end, even on step error

    let step = step?; // propagate transport/crash error (item 2)
    if step.status != "completed" {
        anyhow::bail!("cline slot did not complete: status={} error={:?}", step.status, step.error);
    }
    let decision = step.decision_json
        .ok_or_else(|| anyhow::anyhow!("cline slot completed without calling submit_decision"))?;

    Ok(LlmResponse {
        content: vec![ContentBlock::ToolUse {
            name: "submit_decision".into(),
            input: serde_json::from_str(&decision)?,
            id: "submit_decision".into(),
        }],
        stop_reason: StopReason::ToolUse,
        input_tokens: step.usage.input_tokens,
        output_tokens: step.usage.output_tokens,
    })
}
```
Add the new `SlotInput` fields used above (`cline_client: Option<&AgentClient>`, `provider_entry`, `api_key`, `run_id`, helper methods) to `execute.rs`'s `SlotInput`. Confirm the existing decision parser accepts a `ToolUse{name:"submit_decision"}` block — if it currently parses from a JSON content block, adapt the parser or have `execute_slot_cline` emit the matching block variant. (Match the *existing* shape; do not invent a new one.)

- [ ] **Step 4: Run — PASS.** Step 5: Commit `feat(stage1): execute_slot_cline backed by AgentClient`.

---

### Task 6: Wire the runtime flag into the pipeline

**Files:**
- Modify: `crates/xvision-engine/src/agent/pipeline.rs` (`run_agent_pipeline` + legacy `run_pipeline`)
- Modify: `crates/xvision-engine/src/api/eval.rs` (construct `AgentClient` with event sink when `runtime == Cline`)
- Test: `crates/xvision-engine/tests/cline_pipeline_flag.rs`

- [ ] **Step 1: Failing test** — run a full pipeline with `runtime = Cline` (mock sidecar) and assert a `TraderDecision` comes out; run again with `runtime = LlmDispatch` (mock dispatch) and assert the existing path still works. Both must produce a parseable decision.

- [ ] **Step 2: Run — FAIL** (pipeline does not branch on runtime).

- [ ] **Step 3: Implement** — in `run_agent_pipeline`, per slot: `match input.runtime { AgentRuntime::Cline => execute_slot_cline(slot_input).await?, AgentRuntime::LlmDispatch => execute_slot(slot_input).await? }`. Thread `runtime: AgentRuntime` and `cline_client: Option<Arc<AgentClient>>` through `PipelineInputs`. In `eval.rs`, when `runtime == Cline`, spawn/obtain the shared `AgentClient` via `AgentClient::spawn_with_event_sink(...)` (Task 7 wires the sink) and pass it down; when `LlmDispatch`, keep `build_eval_dispatch` exactly as today.

- [ ] **Step 4: Run — PASS (both arms).** Step 5: Commit `feat(stage1): pipeline selects Cline vs LlmDispatch by runtime flag`.

---

### Task 7: Operational visibility — event sink + `trajectory_mode` (item 3)

**Files:**
- Create: `crates/xvision-engine/migrations/0NN_run_trajectory_mode.sql` (next free 3-digit index; check `ls crates/xvision-engine/migrations | sort | tail -1`)
- Modify: `crates/xvision-observability/src/events.rs` + `sqlite.rs` (persist `trajectory_mode`)
- Modify: `crates/xvision-cli/src/commands/run/inspect.rs`
- Modify: `frontend/web/src/api/types-agent-runs.ts`, `frontend/web/src/routes/agent-runs-detail.tsx:156`, `frontend/web/src/features/agent-runs/RunStatusStrip.tsx`
- Test: `crates/xvision-engine/tests/cline_observability_live.rs` + a frontend type check

- [ ] **Step 1: Failing test** — run a Cline pipeline with an event sink wired to a `SqliteRecorder` over a temp DB; assert `agent_runs` has a row for the run with `trajectory_mode = 'live'` and that `model_calls`/`tool_calls` rows exist (events reached the recorder).

- [ ] **Step 2: Run — FAIL** (`trajectory_mode` column absent; live path not sink-wired).

- [ ] **Step 3: Implement**
  - Migration: `ALTER TABLE agent_runs ADD COLUMN trajectory_mode TEXT NOT NULL DEFAULT 'live';` (plus `.down.sql`). Declare the sibling fields now so Stages 2–3 fill them: `ALTER TABLE agent_runs ADD COLUMN replay_hit_ratio REAL; ADD COLUMN dropped_events INTEGER NOT NULL DEFAULT 0; ADD COLUMN recovery_reason TEXT;`.
  - Recorder: write `trajectory_mode = "live"` on `RunStarted` for Cline runs.
  - `AgentClient::spawn_with_event_sink` is the spawn variant used in Task 6 — confirm its events reach `SqliteRecorder` via `RunEventBus`.
  - CLI: `xvn run inspect <id>` prints `Trajectory mode: live`.
  - Frontend: add `trajectory_mode`, `replay_hit_ratio`, `dropped_events`, `recovery_reason` to `AgentRunSummary`/`AgentRunDetail`; **re-enable** the disabled badge at `agent-runs-detail.tsx:156` to render the mode; add the mode to `RunStatusStrip`. **No popups** — inline badge + strip only, per the project rule.

- [ ] **Step 4: Run — PASS;** `cd frontend/web && npm run typecheck` clean. Step 5: Commit `feat(stage1): live runs surface trajectory_mode via event sink + UI`.

---

### Task 8: Failure + recovery (item 2)

**Files:**
- Modify: `crates/xvision-engine/src/agent/execute_cline.rs` (error mapping)
- Test: `crates/xvision-engine/tests/cline_failure_recovery.rs`

- [ ] **Step 1: Failing tests** (three):
  1. **Crash mid-step** — kill the sidecar process during a step; assert `execute_slot_cline` returns an `Err` whose message identifies a sidecar/transport failure, and that the pipeline marks the cycle failed (not a silent empty decision).
  2. **Idempotency** — call `start_run` twice with the same `run_id`; assert the second is rejected/deduped (no double execution). (Sidecar: `store.ts` keys sessions by `run_id`; assert a duplicate `start_run` errors or is a no-op.)
  3. **Partial-cycle recovery** — a 2-slot pipeline where slot 2 crashes; assert slot 1's output is preserved in the run record and the cycle is marked failed at slot 2, not corrupted.

- [ ] **Step 2: Run — FAIL.**

- [ ] **Step 3: Implement** — map transport/`end_run` errors to a typed `ClineRuntimeError` (crashed / budget / protocol); ensure `run_id` duplicate detection in the sidecar `store.ts` returns a JSON-RPC error that `AgentClient.start_run` surfaces; ensure the pipeline records upstream slot outputs before invoking the next slot so a later crash cannot erase them. Add a code comment marking **live-vs-replay divergence handling as a Stage 3 obligation** (cross-ref the umbrella) so it is tracked.

- [ ] **Step 4: Run — PASS.** Step 5: Commit `feat(stage1): typed Cline failure + recovery semantics`.

---

### Task 9: Flip default to Cline; verify exit criteria

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (flip default)
- Test: update Task 2's default test

- [ ] **Step 1: Update the default test** to expect `AgentRuntime::Cline` and assert `LlmDispatch` is still reachable via the flag (`"llm-dispatch".parse()` works).

- [ ] **Step 2: Flip** `#[default]` to `Cline`. `LlmDispatch` remains a valid flag value (the fallback per invariant 6).

- [ ] **Step 3: Exit verification** — run a real (or mock-sidecar) live/forward-paper cycle end-to-end; confirm the `TraderDecision` is produced via Cline, the run appears in the agent-runs UI with `trajectory_mode = live`, and selecting `runtime = llm-dispatch` still routes through the old path.

- [ ] **Step 4: Commit** `feat(stage1): default live runtime to Cline; LlmDispatch is flag-gated fallback`.

---

## Self-Review

- **Spec coverage (Stage 1 scope):** provider config → Cline ids (Task 1 ✓), `start_run/step/end_run` into the pipeline (Tasks 5–6 ✓), `submit_decision` lifecycle (Tasks 3–4 ✓), `xvision-mcp` indicators as Cline tools (Task 4 ✓), observability via event sink (Task 7 ✓), `LlmDispatch` demoted to flag fallback (Tasks 2, 6, 9 ✓).
- **Item 2 ✓** Task 8 (crash boundary, idempotency, partial recovery); divergence explicitly deferred to Stage 3 with a tracked cross-ref.
- **Item 3 ✓** Task 7 (CLI + dashboard + artifacts; sibling fields declared for Stages 2–3); no-popup rule honored.
- **Item 5 ✓** Task 1 (matrix doc + typed abort on unmapped provider).
- **Placeholder scan:** The Cline `providerId` strings are flagged as an explicit *audit* step with concrete fill-in points, not a silent TBD. All other code is complete.
- **Type consistency:** `StepResult.decision_json` (Task 3 Node / Task 4 Rust) and `submit_decision` tool name are used identically in Tasks 3–5. `SlotInput` new fields introduced in Task 5 are the ones referenced in Task 6.
- **No-cargo discipline:** every `cargo test` step is annotated to run from a worktree with a per-stage `CARGO_TARGET_DIR`, never the shared main checkout.
