# Plan 1 — Data Integrations and Capability Surface for Agents

> **For Hermes:** use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this task-by-task.

**Goal:** Make external market/research integrations available to xvision agents as explicit, *verified* capabilities so strategies can request data from providers like Nansen, Elfa, LunarCrush, Token Terminal / TokenMetrics, and other discovered onchain or market-data sources without hard-coding vendor logic into prompts.

**Architecture:** Split the problem into four layers: (1) provider discovery and verification that establishes what is actually available, (2) provider adapters that know how to talk to a specific vendor or API, (3) a capability registry that describes what each adapter can provide, and (4) agent-facing tools that return typed evidence packs with provenance and freshness. The implementation should be *API-first* and then surfaced through thin CLI, dashboard, and read-only MCP wrappers, so agents can discover and request capabilities without the product becoming provider-specific. The plan should treat Orderly / Bybit on-chain or market-derived data as *possible providers* rather than assumptions; verification and discovery come first, then normalized capabilities, then agent exposure, with operator-visible cost / quota metadata where external APIs impose usage limits.

**Tech Stack:** Rust 2021 workspace (`xvision-engine`, `xvision-mcp`, `xvision-cli`, `xvision-dashboard`), serde, SQLite, existing provider config surfaces, and the current agent capability system.

**Reference docs:**
- `crates/xvision-mcp/src/lib.rs` — current stateless MCP shape
- `crates/xvision-engine/src/agent/dispatch_capability.rs` — agent capability dispatch path
- `crates/xvision-engine/src/strategies/*` — strategy/agent metadata surfaces
- `crates/xvision-dashboard/src/routes/settings/providers.rs` — provider configuration surfaces
- `crates/xvision-dashboard/wiki/*` — operator docs that may need updated capability descriptions

---

## Why this matters

The gap analysis shows that xvision already has indicator tools, strategy graphs, and a capability-first direction, but it still lacks a clean path for richer third-party data to become a first-class agent input. Right now, a lot of that information is either:

- implied in prompts,
- hidden in external tooling,
- or not represented as a stable agent capability at all.

This plan makes the boundary explicit:

- **capability** = a stable, typed thing an agent can ask for or consume
- **provider** = the specific vendor/API/runtime that fulfills the request
- **evidence pack** = the normalized output with provenance, timestamps, and source metadata

That keeps the system comparable across vendors and avoids baking vendor names into every agent prompt.

### Grounded constraints from repo review

- xvision already has a **Settings → Providers** surface, so this plan should extend that surface where possible instead of inventing a parallel provider system.
- External data API costs / quota pressure are not modeled today, so the plan needs an explicit place to carry that metadata.
- A data-capability registry is not yet a first-class UI/CLI object, so discovery and refresh need visible operator surfaces.
- Any exchange-adjacent onchain sources should remain **discovery targets**, not assumed data platforms.

---

## Proposed shape

### Capability families

Treat each data family as a capability family, not just a provider integration:

- **onchain / wallet intelligence**
  - Nansen-style whale / smart-money / exchange-flow signals
- **social / attention intelligence**
  - LunarCrush-style engagement, social dominance, trending assets
- **protocol / fundamental crypto intelligence**
  - Token Terminal / TokenMetrics-style protocol revenue, fees, active users, valuation proxies
- **exchange / execution-adjacent data**
  - market data, funding, OI, liquidations, basis, or any verified onchain/market hybrid surface
- **derived evidence packs**
  - normalized summaries for researcher / trader / risk / portfolio bots

### Access surfaces

Prefer a layered exposure model:

1. **Engine capability registry** — authoritative internal source of what exists
2. **Read-only MCP tools** — agent-friendly fetch / inspect tools
3. **CLI commands** — operator-friendly discovery and debugging
4. **API endpoints** — dashboard / eval / strategy surfaces can use directly

The key rule: agent-facing surfaces should call into the same underlying capability registry so the user does not get three slightly different versions of the same data.

### Evidence pack contract

Every capability response should return a structured evidence pack containing:

- capability name
- provider name
- request parameters
- timestamp / freshness window
- source IDs / URLs / hashes when available
- confidence / verification status
- cost / quota / rate-limit metadata when available
- normalization notes
- raw payload excerpt or digest

This makes the downstream agent output auditable and lets later compare/eval surfaces explain why one strategy got different information than another.

---

## Implementation plan

### Task 1: Define the capability registry contract

**Objective:** Create the stable typed surface that says what data capabilities exist and how agents request them.

**Files:**
- Create or modify: `crates/xvision-engine/src/data/capabilities/*`
- Modify: `crates/xvision-engine/src/agent/*`

**Work:**
- Add a `DataCapability` or equivalent enum for data families.
- Add a typed request/response shape for capability calls.
- Keep agent optimizer capabilities separate from data capabilities.
- Make unsupported requests fail loudly and clearly.

**Verification:**
- Capability enumeration is serializable.
- Unsupported capability requests return typed errors.
- Existing non-data capabilities keep working unchanged.

---

### Task 2: Add provider adapters behind a normalization layer

**Objective:** Wrap external vendors in provider-specific adapters that emit normalized evidence packs, while extending the existing provider settings surface where needed.

**Files:**
- Create or modify: `crates/xvision-engine/src/data/*`
- Modify: `crates/xvision-engine/src/api/*`
- Modify: `crates/xvision-dashboard/src/routes/settings/providers.rs`

**Work:**
- Define adapters for vendor-backed data sources.
- Normalize vendor-specific fields into xvision-native shapes.
- Preserve raw metadata where possible for provenance.
- Handle missing / partial / stale responses explicitly.
- If the existing provider settings form cannot express data-provider credentials, scopes, and rate limits cleanly, add a data-provider subsection rather than a separate parallel system.

**Verification:**
- Each adapter returns a typed evidence pack.
- The evidence pack clearly records source and freshness.
- Partial data does not silently masquerade as full data.
- Operators can configure data-provider credentials and rate limits without exposing secret values.

---

### Task 3: Add provider discovery and verification for onchain and market-data sources

**Objective:** Determine which onchain and market-data surfaces are actually available and expose only verified capabilities.

**Files:**
- Create or modify: `crates/xvision-engine/src/data/providers/*`
- Modify: `crates/xvision-execution/src/*` if execution-adjacent data is shared
- Modify: `crates/xvision-cli/src/commands/*`
- Modify: `crates/xvision-dashboard/src/routes/settings/providers.rs`

**Work:**
- Add a discovery step that checks available endpoints / metadata.
- Record whether the provider offers market data, on-chain-derived data, or merely execution metadata.
- Persist the discovery result in a refreshable artifact or cache so capability availability survives process restarts.
- Do not hard-code a claim that onchain data exists until verified.
- Expose the result as capability metadata, not as a vague prompt note.

**Verification:**
- The system can say “supported,” “unsupported,” or “unknown” for each capability family.
- Discovery output is visible to operators.
- Agents only see capabilities that are actually available.
- A refresh command can re-run discovery and update the stored status.

---

### Task 4: Expose data capabilities through MCP and CLI

**Objective:** Give agents and operators a consistent way to request the new data capabilities, and make capability discovery visible in the dashboard as well as MCP / CLI.

**Files:**
- Modify: `crates/xvision-mcp/src/tools.rs`
- Modify: `crates/xvision-mcp/src/lib.rs`
- Modify: `crates/xvision-cli/src/commands/*`
- Modify: `crates/xvision-dashboard/src/routes/*`
- Modify: any MCP tool inventory / parity test files already used in the repo

**Work:**
- Add read-only tools for listing capabilities and fetching evidence packs.
- Add CLI commands for capability discovery and debugging.
- Add dashboard views for capability discovery, verified status, and refresh results.
- Keep the transport thin: the same registry should serve MCP, CLI, and API.
- Update the tool inventory / parity checks so new MCP tools are not silently unregistered.
- Make failure cases self-explanatory.

**Verification:**
- An agent can ask for a capability by name and get a typed response.
- An operator can inspect what providers are available.
- An operator can inspect capability status and last refresh time in the dashboard.
- No secrets or provider credentials leak through the response surface.
- The MCP tool registry and the code path stay in sync.

---

### Task 5: Wire capability use into agent prompts and strategy summaries

**Objective:** Let agents know what they can request without embedding vendor-specific logic in every prompt.

**Files:**
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`
- Modify: `crates/xvision-intern/src/prompt.rs`
- Modify: `crates/xvision-trader/src/prompt.rs`
- Modify: `crates/xvision-dashboard/src/routes/*` where strategy summary data is rendered

**Work:**
- Add a compact capability summary to the prompt context.
- Show capability availability, provenance, and freshness.
- Avoid dumping provider implementation details into prompts.
- Keep the prompt text stable across providers.

**Verification:**
- Agents can see which capabilities they can request.
- Prompt changes do not break existing strategies.
- Provider details stay out of the public-facing prompt unless needed for debugging.

---

### Task 6: Add marketplace / eval disclosure for provider-backed evidence

**Objective:** Make it visible when a strategy depends on non-standard or provider-specific data.

**Files:**
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval_runs.rs`

**Work:**
- Record provider provenance on run artifacts / evidence packs.
- Record cost / quota / rate-limit pressure on run artifacts or evidence packs when the provider exposes it.
- Display whether a strategy used vendor-backed evidence without hard-wiring provider names into the strategy definition itself.
- Add compare/eval labels for “standard-only” vs “external-data-assisted.”

**Verification:**
- The UI can tell the user when a strategy is built on external data.
- Compare/eval results show capability provenance.
- Provider-backed runs remain auditable.

---

### Task 7: Add tests and failure-mode coverage

**Objective:** Make sure capability discovery is robust and degrades safely.

**Files:**
- Add tests alongside the capability, provider, and MCP modules

**Work:**
- Test supported/unsupported/unknown capability states.
- Test provider adapter normalization.
- Test that missing verification data falls back to `unverified` rather than crashing.
- Test that agents can request capabilities through the chosen surface.

**Verification:**
- Test suite covers both happy path and degraded provider cases.
- No hidden assumptions about vendor availability.

---

## Acceptance criteria

- Agents can request external data through a stable capability surface.
- The system can integrate vendor data without hard-coding vendor logic into prompts.
- Exchange-adjacent data is treated as a discovered capability, not an assumption.
- Capability status is visible and refreshable from the operator UI.
- Capabilities are visible via MCP and CLI.
- Evidence packs carry provenance and freshness.
- Evidence packs carry cost / quota metadata when available.
- Missing verification data degrades safely to `unverified` / `unknown`.

---

## Risks and concerns

- Vendor APIs may not expose the same provenance / hashes.
- Some data may be market-derived rather than truly on-chain.
- If prompts know too much about providers, the system becomes brittle.
- If capability names are too broad, agents may over-request or hallucinate coverage.

Mitigation:
- Keep capability names narrow.
- Keep provider-specific details behind adapters.
- Make provenance explicit in the evidence pack.
- Prefer typed errors over silent fallback.

---

## High-level recommendation

The best implementation path is: **registry first, provider adapters second, agent surfaces third, marketplace disclosure fourth**. That gives xvision a clean way to absorb future vendor integrations without turning every strategy into a pile of provider-specific prompt hacks.
