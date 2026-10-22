# Autoresearcher Research Synthesis and Plan Revision Implementation Plan

> **For agentic workers:** REQUIRED: Use `superpowers:subagent-driven-development` (if subagents available) or `superpowers:executing-plans` to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the existing Autoresearcher research into a tighter set of plan/spec revisions that keep the useful parts — strategy handoff, multi-object meta-strategies, lineage/provenance, judge/canary surfaces, and dashboard review — while keeping Slack/email approval out of the core path.

**Architecture:** This is a doc-and-plan synthesis track, not a code track. First we define the terms that matter and map them onto xvision’s existing bundle / lineage vocabulary. Then we revise the three Autoresearcher plans so they stay aligned: AR-1 keeps the substrate/provenance layer, AR-2 keeps the cycle/judge/eval loop and adds multi-object coordination, and AR-3 keeps the dashboard/provenance surfaces. Any human review that survives should stay local and explicit inside xvision.

**Tech Stack:** Markdown plan/spec docs, the Autoresearcher design/spec docs, existing Autoresearcher plan docs, and the terminology lock / operator-facing handoff docs.

---

## Why this plan exists

The research is useful, but it needs triage.

### Keep
- strategy handoff as an explicit parent → child transition
- meta-strategies that combine multiple objects/bundles
- lineage and provenance for every handoff, mutation, and decision
- cycle seals / auditable summaries
- judge, canary, inversion, and diversity checks as internal quality controls
- dashboard surfaces that let operators inspect the object graph and compare alternatives
- local in-app review/sign-off where a human gate is still needed

### Drop from the core path
- Slack approval flows
- email approval flows
- any external messaging-based approval system
- hidden review paths that cannot be replayed from xvision state

### Why this split matters
- External approvals add latency and state drift without improving the Autoresearcher core.
- The durable substrate is bundle/lineage/provenance, not notification plumbing.
- Strategy handoff and meta-strategies are the useful abstractions; they let one strategy hand off to another and let a larger strategy coordinate multiple objects.

---

## Terms to keep the plan grounded

Before editing the Autoresearcher plans, lock down a minimal mapping from the new research terms back to the existing AR vocabulary:

- **Strategy handoff** → an explicit parent/child transition between strategy bundles, recorded in lineage and shown in the dashboard.
- **Meta-strategy** → a bundle or cycle artifact that references multiple constituent objects/bundles and is reviewed as a set.
- **Object** → the existing strategy / scenario / agent / eval entity already tracked elsewhere in xvision.
- **Provenance** → the lineage record, cycle seal, and any per-object metadata needed to explain why a result exists.
- **Review / sign-off** → local xvision dashboard or CLI state, not Slack or email.

This plan should not invent a parallel object graph if the existing bundle/lineage model can carry the same meaning.

---

## Proposed changes to the existing Autoresearcher plans

### AR-1 — Mutator + lineage + gate + seal
Keep the substrate scope, but revise it so the core unit is not just a single mutated bundle.

Add:
- strategy handoff metadata
- multi-object provenance on lineage nodes and cycle seals
- explicit parent/child references that can later be consumed by AR-2 and AR-3

Remove or de-emphasize:
- any Slack/email approval language
- any external approval path that is not replayable from xvision state

### AR-2 — Cycle orchestrator + judge + canary + inversion + diversity
Keep the full cycle/judge machinery, but make it work on related object sets rather than a single linear lane.

Add:
- selection of multiple parent objects as one meta-strategy context
- handoff output from one strategy to another strategy
- per-object lineage references in cycle results
- judge input/output that can describe a multi-object bundle without hiding which object contributed what

### AR-3 — Dashboard + SSE + ladder UI
Keep the dashboard, but make it visibly about handoffs and multi-object review, not only live-cycle ticking.

Add:
- handoff chain inspection
- multi-object bundle comparison
- provenance summaries for each object in the set
- local review / sign-off state if needed

---

## Files to touch

- Modify: `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md`
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-2-cycle-judge-evals.md`
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-3-dashboard.md`
- Modify: `docs/design/2026-05-27-autoresearcher-designer-reference.md`
- Modify: `docs/design/2026-05-27-autoresearcher-cli-rename-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-frontend-rename-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-sse-registry-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-spec-amendment-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-skills-docs-sweep-handoff.md`

---

## Task 1: Define the new terms without leaving the existing AR vocabulary

**Files:**
- Modify: `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md`

- [ ] **Step 1: Add a failing terminology note**

Write down where the existing research only speaks in single-bundle terms, and flag the new terms that need a mapping: strategy handoff, meta-strategy, and multi-object provenance.

- [ ] **Step 2: Map the new terms to existing AR concepts**

Write explicit definitions that map handoff and meta-strategy back to bundle, lineage, parent, child, and cycle-seal vocabulary.

- [ ] **Step 3: Remove ambiguous approval language from the core definition**

Keep review/sign-off local and explicit. Do not introduce Slack or email approval into the canonical Autoresearcher vocabulary.

- [ ] **Step 4: Re-read the spec and confirm the terms are now anchored**

Expected result: the revised terminology can be used consistently in the plans without inventing a parallel system.

---

## Task 2: Revise AR-1 to carry handoff and multi-object provenance

**Files:**
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md`
- Modify: `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`

- [ ] **Step 1: Write the failing plan delta**

Annotate the AR-1 plan where it is too single-bundle-centric and where it needs to mention handoff bundles and multi-object provenance.

- [ ] **Step 2: Update the plan scope**

Rewrite the plan summary so AR-1 clearly owns the substrate: content-addressed bundle lineage, gate, seal, and the data needed for future handoff / meta-strategy work.

- [ ] **Step 3: Keep the substrate narrow**

Do not pull orchestration semantics into AR-1 beyond the minimal parent/child/provenance fields needed to make downstream plans possible.

- [ ] **Step 4: Re-run the plan review mentally**

The result should still read like a substrate plan, not a full orchestrator plan.

---

## Task 3: Revise AR-2 for multi-object cycles and handoff chains

**Files:**
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-2-cycle-judge-evals.md`
- Modify: `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`

- [ ] **Step 1: Write the failing cycle-orchestration note**

Mark the places where AR-2 still assumes a single linear strategy lane instead of a related object set.

- [ ] **Step 2: Add multi-object selection to the plan**

Update the plan so the orchestrator can select multiple parent objects as one meta-strategy context.

- [ ] **Step 3: Add handoff-chain language to the judge and eval steps**

Make it clear that one strategy can hand off to another strategy, and that the judge must preserve per-object provenance in the final result.

- [ ] **Step 4: Preserve the quality-control checks**

Keep canary, inversion, and diversity in the loop. Those are still useful even when the strategy is multi-object.

- [ ] **Step 5: Re-read the plan for coherence**

Expected result: AR-2 now reads like a multi-object orchestrator, not just a single-bundle mutation loop.

---

## Task 4: Revise AR-3 to expose handoff review and provenance

**Files:**
- Modify: `docs/superpowers/plans/2026-05-09-autoresearcher-3-dashboard.md`
- Modify: `docs/design/2026-05-27-autoresearcher-frontend-rename-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-sse-registry-handoff.md`

- [ ] **Step 1: Write the failing dashboard scope note**

Point out where the dashboard plan only describes a live cycle ticker and not a handoff/provenance review surface.

- [ ] **Step 2: Add handoff-chain inspection to the plan**

Update the plan so operators can inspect a handoff chain and compare a meta-strategy against its constituent objects.

- [ ] **Step 3: Add local review state, not external approval channels**

If the dashboard needs review/sign-off, keep it local to xvision. Do not add Slack/email workflows.

- [ ] **Step 4: Re-run the plan review mentally**

The result should read like a provenance-first operator dashboard, not an alerting system.

---

## Task 5: Align the operator vocabulary docs with the revised plans

**Files:**
- Modify: `docs/design/2026-05-27-autoresearcher-designer-reference.md`
- Modify: `docs/design/2026-05-27-autoresearcher-cli-rename-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-frontend-rename-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-sse-registry-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-spec-amendment-handoff.md`
- Modify: `docs/design/2026-05-27-autoresearcher-skills-docs-sweep-handoff.md`

- [ ] **Step 1: Add the revised vocabulary to the docs**

Make sure the operator-facing docs now explain:
- strategy handoff
- meta-strategy
- lineage
- cycle summary
- local review / sign-off

- [ ] **Step 2: Remove Slack/email approval references**

Delete or de-emphasize any copy that suggests the operator should approve Autoresearcher work through Slack or email.

- [ ] **Step 3: Keep the plain-language surface**

Preserve the operator-friendly naming style established by the terminology lock. Keep the mental model simple: handoff, review, lineage, result.

- [ ] **Step 4: Re-run a doc review pass**

Confirm the docs still match the revised Autoresearcher roadmap and do not reintroduce jargon that the audit already rejected.

---

## Acceptance criteria

- The Autoresearcher roadmap explicitly keeps strategy handoff and meta-strategy support.
- Slack/email approval does not appear in the core implementation path.
- Multi-object strategies are a first-class concept in the plan, not a side note.
- Lineage, cycle seals, judge/canary/inversion/diversity, and dashboard provenance remain in scope.
- The operator can review everything locally inside xvision.
- The rewritten plan set is coherent across AR-1, AR-2, and AR-3.

---

## Notes / risks

- Don't let approval plumbing hijack the roadmap. The useful abstraction is handoff + review, not Slack/email notification.
- Don't collapse a meta-strategy back into a single object just to make the implementation easier.
- Don't overfit the plan to one UI view; the same handoff model should support CLI, dashboard, and future automation.
- Keep the plan focused on the durable parts of the research. Anything that is just notification friction should stay out.
