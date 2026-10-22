# Plan 3 — Loose-Framework Bots for Research, Risk, and Portfolio

> **For Hermes:** use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this task-by-task.

**Goal:** Add loosely framed bots for research, risk analysis, and portfolio analysis that are orchestrated mostly through visible system prompts and strategy wiring, while keeping the deterministic risk layer as the hard cap and preserving comparable eval behavior.

**Architecture:** Treat the bots as transparent prompt/system-prompt frameworks rather than invisible application logic. The user should be able to see what each bot is supposed to do, what inputs it receives, and what it is *not* allowed to override. The hard risk cap remains in deterministic code; the risk analysis bot only advises within that cap. The “researcher” can simply be a cheekier name for the current Intern role, but the plan should preserve that it is still an evidence-preparation bot rather than a hidden planner.

**Tech Stack:** Rust 2021 workspace (`xvision-engine`, `xvision-intern`, `xvision-trader`, `xvision-core`, `xvision-risk`, `xvision-dashboard`), prompt templates, existing agent slots, compare/eval surfaces, and the current risk primitives.

**Reference docs:**
- `architecture.md` — deterministic risk and multi-stage pipeline
- `crates/xvision-core/src/risk.rs` — hard cap / risk primitives
- `crates/xvision-risk/*` — deterministic risk evaluation layer
- `crates/xvision-intern/src/prompt.rs` — intern prompt surface
- `crates/xvision-trader/src/prompt.rs` — trader prompt surface
- `crates/xvision-engine/src/agent/pipeline.rs` — how slots are resolved
- `docs/freqtrade-graphs-and-metrics.md` — exit reason / lifecycle metrics language
- `crates/xvision-dashboard/wiki/xvision-vs-agentic-trading-research.md` — gap analysis bridge

---

## Why this matters

The gap analysis makes clear that the report’s “Researcher / Trader / Risk / Execution” shape is useful, but xvision should not hide critical behavior under the hood. The user wants bots that are:

- visible,
- prompt-driven,
- easy to understand,
- and easy to compare against mechanistic or fully agentic strategies.

That means the bot surface should feel like a set of **loose frameworks**, not a black-box framework that secretly does a lot of hidden orchestration. The risk cap must remain a hard deterministic boundary so the bot can advise, but not exceed platform-level safety.

---

## Proposed shape

### Bot roles

Use a small, legible set of roles:

- **Researcher** — the current Intern with a cheekier / friendlier name; prepares evidence and structured observations
- **Trader** — chooses the trade direction and sizing within the allowed context
- **Risk Analyst** — evaluates risk, flags hazards, and advises conservatism
- **Portfolio Analyst** — summarizes portfolio exposure, concentration, correlation, and allocation pressure
- **Execution** — remains deterministic and should not become an LLM bot

### Hard cap vs bot-level risk

The split should be explicit:

- **hard cap** = deterministic system risk envelope that can veto or clamp decisions
- **risk bot** = prompt-driven analyst that recommends or warns *inside* the cap

The risk bot should never be able to widen the cap by itself. It can only:

- recommend smaller size,
- recommend flat,
- explain why risk is elevated,
- or tag the decision for review.

### Loose-framework principle

The bots should be mostly defined by:

- their system prompt,
- their inputs,
- their output schema,
- and the strategy graph wiring.

The under-the-hood programming should stay minimal and visible. If we need hidden logic, that should be kept in the deterministic risk/execution layers, not the bot bodies.

---

## Implementation plan

### Task 1: Define the bot vocabulary and user-facing naming

**Objective:** Make the bot set explicit and easy to understand in the UI and docs.

**Files:**
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/wiki/xvision.md`

**Work:**
- Introduce a visible bot naming surface.
- Allow “Researcher” as a *display alias* for Intern without changing the underlying canonical role semantics.
- Add Risk Analyst and Portfolio Analyst as prompt-driven roles or templates.
- Keep Execution non-agentic.

**Verification:**
- Users can see the bot names in strategy detail.
- The aliasing does not break existing canonical role logic.
- Execution remains deterministic.

---

### Task 2: Make the risk cap a deterministic hard boundary

**Objective:** Ensure the risk bot cannot override or silently widen the platform risk envelope.

**Files:**
- Modify: `crates/xvision-core/src/risk.rs`
- Modify: `crates/xvision-risk/*`
- Modify: `crates/xvision-engine/src/eval/executor/*`
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`

**Work:**
- Preserve the hard cap in deterministic code.
- Define exactly what the risk bot can influence: warnings, annotations, or clamp suggestions only.
- Make the risk bot strictly advisory-only with a read-only output schema.
- Ensure the risk bot only operates *within* the cap.
- Make vetoes and clamps visible in traces.

**Verification:**
- The system can prove the cap is still enforced outside the bot.
- The risk bot cannot increase maximum allowed exposure.
- Veto / clamp reasons remain observable.

---

### Task 3: Add the portfolio bot as a transparent summarizer

**Objective:** Give strategies a legible portfolio-awareness layer without burying logic.

**Files:**
- Modify: `crates/xvision-core/src/trading.rs`
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`
- Modify: `crates/xvision-trader/src/prompt.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval_runs.rs`

**Work:**
- Build a portfolio summary prompt surface.
- Include exposure, concentration, cash, drawdown, and open-position context.
- Keep the portfolio bot strictly summary-only: no trade directives, no sizing advice, no action selection.
- Make it clear that the portfolio bot is advisory.
- Keep the summary shape stable enough for eval comparison.

**Verification:**
- The portfolio bot can be inspected independently.
- The trader can consume portfolio context without hidden logic.
- Compare views can display portfolio-aware decisions.

---

### Task 4: Keep the Researcher as a cheekier Intern alias

**Objective:** Preserve the evidence-preparation role but make the UX friendlier and more aligned with the user’s wording.

**Files:**
- Modify: `crates/xvision-intern/src/prompt.rs`
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`

**Work:**
- Add an alias label for Intern in the UI and prompt templates.
- Keep the underlying semantics unchanged.
- Make the researcher output evidence, not hidden trade planning.

**Verification:**
- The UI can present the role as Researcher without breaking role plumbing.
- Outputs remain evidence-first.
- No new invisible planning logic appears.

---

### Task 5: Add optional loose orchestration via prompts, not hidden logic

**Objective:** Let the strategy graph orchestrate these bots through visible prompt instructions and slot wiring.

**Files:**
- Modify: `crates/xvision-engine/src/agent/pipeline.rs`
- Modify: `crates/xvision-engine/src/strategies/*`
- Modify: `crates/xvision-dashboard/src/routes/authoring.tsx`

**Work:**
- Allow a strategy to say which bot feeds which other bot using an explicit, inspectable DAG / edge list in strategy config.
- Keep orchestration visible in strategy detail and eval traces.
- Avoid embedding a separate hidden planning engine.
- Make the prompt instructions show what each bot should do with its inputs.

**Verification:**
- A user can see the bot chain.
- Bot chaining works without undocumented internal behavior.
- The prompt text reflects the orchestration.

---

### Task 6: Surface concerns and quick wins in the product

**Objective:** Make the limitations and intended uses obvious so users do not mistake the bot layer for autonomous control.

**Files:**
- Modify: `crates/xvision-dashboard/wiki/*`
- Modify: `frontend/web/src/routes/*`
- Modify: `docs/freqtrade-graphs-and-metrics.md`

**Work:**
- Show when a bot is advisory vs decision-making.
- Show whether a bot was constrained by the hard cap.
- Document quick wins such as:
  - better risk explanations
  - better portfolio summaries
  - clearer compare-by-role views
- Document concerns such as prompt drift and hidden behavior creep.

**Verification:**
- The product makes the boundaries clear.
- Users can see why a bot said “no.”
- The UI doesn’t imply the risk bot can override the system cap.

---

### Task 7: Add tests around role semantics and cap enforcement

**Objective:** Prevent future regressions where the bot layer becomes more powerful than intended.

**Files:**
- Add tests adjacent to the touched prompt/pipeline/risk modules

**Work:**
- Test that Researcher aliasing does not change semantics.
- Test that risk bots cannot widen the cap.
- Test that portfolio summaries are produced and routed correctly.
- Test that compare output still distinguishes advisory vs deterministic outcomes.

**Verification:**
- The bot roles remain loose and understandable.
- The cap remains hard.
- Tests cover the key safety boundary.

---

## Concerns, quick wins, and future-facing considerations

### Concerns
- Risk bot prompt drift can create false confidence.
- A portfolio bot can become a second trader if we let it reason too freely.
- The more bot roles we add, the easier it is to hide logic in prompts.
- If the cap is not visually obvious, users may assume the bot can do more than it can.

### Quick wins
- Rename Intern to Researcher in the UI as a friendly alias.
- Add a portfolio summary card to existing strategy detail and eval views.
- Add a risk explanation panel that says why a decision was clamped.
- Add a “within hard cap” badge to bot outputs.

### Future-facing considerations
- Later we may want a dedicated portfolio manager role or aggregator.
- Later we may want a router to decide which specialist bot to consult.
- Later we may want a compare view that shows bot-role deltas, not just strategy deltas.

---

## Acceptance criteria

- Researcher / Risk Analyst / Portfolio Analyst roles are visible and legible.
- The hard risk cap remains deterministic and non-bypassable.
- The risk bot is advisory only, not a cap override.
- The bot layer stays mostly prompt-driven and transparent.
- The UI and eval views make the bot boundaries obvious.

---

## High-level recommendation

Keep the bot layer intentionally soft, but keep the cap hard. That gives xvision a way to present useful specialized behavior without surrendering the deterministic risk boundary that makes the system safe and comparable.
