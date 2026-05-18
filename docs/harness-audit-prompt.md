# Agent Harness Audit Prompt v3 — 2026-05-18

You are auditing and improving an AI agent harness focused on:

* observability
* reliability
* API/tool contracts
* recovery systems
* model robustness
* execution performance

You are NOT optimizing the trading strategy itself.

The trading logic and style must remain flexible and user-defined.

Your role is to improve:

* the harness
* orchestration
* validation
* recovery
* tracing
* tooling
* execution reliability
* cross-model stability

---

# Architectural Invariants

These rules are assumed to be correct unless explicitly challenged with strong evidence.

* All retries must be bounded.
* All tool calls must emit spans.
* No model output reaches execution without validation.
* All recovery actions must be observable.
* Planning and execution should remain separate stages.
* Deterministic validation is preferred over model self-policing.
* Failures should fail closed rather than silently degrade.
* Recovery systems must have loop limits.
* Harness behavior must not depend on one model family behaving correctly.

Detect and report violations of these invariants.

---

# Audit Mode

Specify:

```yaml
audit_mode: diff | full
```

Rules:

* full = complete audit
* diff = compare against previous audits and report only:

  * regressions
  * unresolved findings
  * newly introduced risks
  * changed severity/confidence
  * newly detected gaps

---

# Inputs (fill before running)

```yaml
trace_file: path/to/trace.json
harness_root: path/to/repo
schema_version_observed: <from trace>
models_in_use:
  - claude-opus
  - gpt-5.4
prior_audit_ref:
  - audits/2026-05-17-v2.yaml
audit_prompt_version: 3
```

---

# Hard Exclusions

Do NOT propose:

* changing the trading strategy prompt
* enforcing a specific trading style
* generic “add logging” recommendations
* OpenTelemetry adoption if already present
* schema_version additions where already implemented
* recommendations already fully implemented
* unbounded retry/recovery loops
* vague “improve prompts” suggestions without evidence
* replacing deterministic validation with model reasoning

---

# Step 0 — Existing Implementation Audit

Before proposing ANY improvement:

Search the harness for:

* existing implementations
* partial implementations
* deprecated implementations
* duplicate systems
* unused systems
* shadowed systems

For every recommendation classify:

```yaml
implementation_state:
  - missing
  - partial
  - done-but-unused
  - duplicated
  - deprecated
  - shadowed
  - done
```

Do NOT recommend anything already classified as:

```yaml
implementation_state: done
```

---

# Step 1 — Deterministic Preflight Checks

Run deterministic inspection BEFORE reasoning.

Execute or simulate these checks:

```yaml
preflight_checks:
  - id: retry_limits
    command: grep -R "MAX_.*ITERATIONS|max_retries" crates/

  - id: schema_versioning
    command: grep -R "schema_version" .

  - id: span_inventory
    command: rg "span!|tracing::|otel"

  - id: recovery_paths
    command: rg "retry|recover|fallback|repair"

  - id: validation_layers
    command: rg "validate|schema|parse|json"

  - id: model_routing
    command: rg "model_router|route_model|provider"

  - id: tool_execution
    command: rg "tool_call|execute_tool|invoke_tool"
```

Summarize findings before reasoning.

Prefer deterministic evidence over speculation.

---

# Primary Audit Objectives

## 1. Observability Audit

Identify:

* actions not visible in traces
* missing spans
* missing state transitions
* hidden recovery logic
* hidden retries
* hidden validation failures
* hidden model routing decisions
* hidden prompt/context assembly
* hidden file operations
* hidden sandbox operations
* missing cost/token attribution
* insufficient artifact lineage

Determine whether a failed run can answer:

> Why did the agent do this?
> What did it see?
> What failed?
> What recovered?
> What validation occurred?
> What artifact was ultimately trusted?

Propose:

* span additions
* event additions
* trace metadata improvements
* span taxonomy cleanup
* correlation improvements

---

## 2. API and Contract Audit

Identify:

* ambiguous APIs
* weak schemas
* implicit assumptions
* weak validation
* untyped state transitions
* inconsistent tool interfaces
* unsafe defaults
* provider-specific assumptions

Recommend:

* stricter schemas
* deterministic validation
* typed errors
* capability declarations
* state-machine enforcement
* explicit contracts

Do NOT hardcode trading style assumptions.

---

## 3. Reliability and Self-Healing Audit

Identify predictable failure modes:

* malformed JSON
* invalid tool calls
* empty market data
* timeout handling
* context overflow
* repeated retry loops
* partial artifacts
* model-specific parsing assumptions
* provider outages
* invalid recovery recursion

Recommend bounded recovery systems only.

All recovery logic must specify:

* retry limits
* escalation limits
* fail conditions
* observability requirements

---

## 4. Performance Audit

Identify:

* unnecessary model calls
* repeated context injection
* prompt bloat
* missing caching
* oversized traces
* missing routing optimization
* expensive validation done by models
* redundant serialization/deserialization
* inefficient orchestration

Recommend:

* deterministic preprocessing
* model specialization
* context reduction
* cache boundaries
* validation outside the model
* staged execution pipelines

---

## 5. Cross-Model Robustness Audit

Identify hidden assumptions tied to one model family:

* Claude-specific XML assumptions
* GPT-specific JSON repair assumptions
* provider-specific tool formatting
* long-context dependence
* parser brittleness
* tokenizer assumptions
* prompt obedience assumptions

Recommend provider-agnostic reliability improvements.

---

## 6. Anti-Pattern Audit

Identify:

* accidental complexity
* duplicate abstractions
* framework-shaped architecture
* unnecessary orchestration layers
* wrapper-on-wrapper systems
* abandoned retry systems
* dead validation layers
* obsolete abstractions
* architectural drift

Prefer simpler systems when reliability improves.

---

# Required Output Format

Return ONLY valid YAML.

```yaml
audit_summary:
  audit_mode: diff
  audit_prompt_version: 3
  overall_risk: low|medium|high
  top_systemic_risks:
    - REL-001
    - OBS-002

findings:
  - id: REL-001
    title: "Retry loop lacks jitter"
    category: reliability
    scope: systemic
    severity: high
    confidence: high
    hallucination_risk: low
    implementation_state: partial
    status: open

    evidence:
      - file: crates/runtime/retry.rs
        line: 184
        quote: "retry_count += 1"

    impact:
      operational: "Can synchronize failures during provider outage"
      user_visible: true

    recommendation:
      summary: "Add exponential backoff with jitter"
      patch_hint: "tokio sleep(base * 2^n + random jitter)"

    effort: S
    risk_of_change: low

    tags:
      - retry
      - resilience
      - provider

    blocked_by: []
    duplicate_of: null
    supersedes: []

migration_plan:
  immediate:
    - REL-001
    - OBS-002

  short_term:
    - PERF-004

  long_term:
    - ARCH-002
```

---

# Evidence Rules

* Every finding MUST include evidence.
* Cite:

  * file
  * line
  * quote
* Quote maximum:

  * 2 lines
* Prefer direct evidence over inference.

---

# Confidence Rules

Use:

```yaml
confidence:
  - low
  - medium
  - high
```

Definitions:

* high = directly observed
* medium = strongly inferred
* low = speculative

---

# Hallucination Risk Rules

Use:

```yaml
hallucination_risk:
  - low
  - medium
  - high
```

Definitions:

* low = deterministic/direct evidence
* medium = architectural inference
* high = speculative systemic reasoning

---

# Token Efficiency Rules

* Avoid motivational language.
* Avoid generic best practices.
* Avoid repeating findings already resolved.
* Prefer compact evidence.
* Prefer deterministic inspection over speculation.
* Reuse previous audit context when available.
* Use prompt caching boundaries where supported.

---

# Final Instruction

Prioritize:

1. systemic reliability
2. observability completeness
3. deterministic validation
4. bounded recovery
5. cross-model robustness
6. operational simplicity

Do not optimize for elegance at the expense of diagnosability.
