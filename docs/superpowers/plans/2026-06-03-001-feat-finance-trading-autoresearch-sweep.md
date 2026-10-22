# Finance-Trading Autoresearch Sweep and XVision Proposal Plan

**Target repos:** `xvision`, `research-wiki`

**Goal:** normalize the finance-trading slice of `awesome-autoresearch`, run xvision-focused DeepSeek research for each repo target, publish the results into a dedicated research-wiki directory, and synthesize the batch into one ranked proposal for xvision improvements.

---

## Summary

This plan turns the finance-trading autoresearch category into a repeatable research bundle for xvision: clean the xvision working tree first, research each unique repo target with the same xvision-focused lens, publish the per-repo notes into a dedicated research-wiki directory, and then collapse the batch into one ranked improvement proposal.

---

## Problem Frame

xvision already has strong trading, strategy, optimizer, and agent-harness surfaces, plus a visible research/wiki layer. The problem is not lack of ideas — it is that the finance-trading autoresearch landscape is broad, uneven, and easy to lose in a pile of ad hoc notes.

The current xvision working tree is also dirty and diverged from `origin/main`, so research should start from a clean baseline before any new output is written. Otherwise the research process risks being anchored to stale or accidental local state.

The plan needs to convert a large external category into a structured internal asset:

- one normalized repo list
- one research pass per repo target
- one consistent xvision lens per pass
- one comparable final ranking across all ideas
- one browseable directory on the research wiki

---

## Requirements

- Reset or clean the `xvision` working tree before the research phase so the work starts from a known baseline.
- Normalize the finance-trading category into a deduped target list.
- Treat repo-like entries as first-class targets and record non-repo anomalies separately.
- Run `100x` CLI research with the `DeepSeek` model for each unique repo target.
- Keep every per-repo research pass focused on xvision reuse:
  - strategy improvement
  - Optimizer / Autoresearcher improvement
  - agent-harness improvement
- Write the research output into a dedicated directory on the research wiki.
- Produce a single synthesis that ranks the ideas by:
  - potential value to xvision
  - token-estimated effort
  - one-sentence reason it matters
- Make the batch discoverable from the existing xvision research surfaces.

---

## Key Technical Decisions

- Use one standard research prompt template for every repo target so the final synthesis can compare like with like.
- Normalize repeated repo names into one research target each, but keep the source category order available for traceability.
- Handle the lone non-repo entry in the source category as an anomaly, not as a normal repo research target.
- Store the per-repo notes in `research-wiki/docs/reports/finance-trading/` with one page per repo target and one batch landing page.
- Keep the final synthesis separate from the per-repo notes so the batch can be reused later without re-reading every note.
- Use a simple scoring model in the synthesis:
  - `value` on a small ordinal scale
  - `token_effort_estimate` as a rough implementation budget
  - `why_useful` as a single sentence
- Group the final recommendations by xvision impact area rather than by source repo so repeated themes collapse into one proposal.
- Link the new batch from the existing xvision research pages so it is easy to find later.

---

## Scope Boundaries

### In scope

- Cleaning the xvision baseline before research
- Running the finance-trading repo sweep
- Writing per-repo research notes
- Writing the batch synthesis and ranking
- Linking the new batch into the research/wiki navigation

### Out of scope

- Direct code changes to xvision runtime behavior
- Reworking the existing xvision architecture
- Changing the finance-trading source category itself
- Turning the research pass into a general web crawl
- Folding unrelated research topics into the same batch

### Assumptions

- The finance-trading category is the authoritative input list.
- The `100x` CLI is the execution path for the research passes.
- The research output directory can live under the research wiki without needing a new repository.
- The non-repo anomaly can be excluded from the repo sweep while still being recorded in the batch notes.

---

## Output Structure

The plan should create a new browseable batch on the research wiki, roughly shaped like this:

- `research-wiki/docs/reports/finance-trading/index.md` — batch landing page and table of contents
- `research-wiki/docs/reports/finance-trading/repos/<repo-slug>.md` — one page per repo target
- `research-wiki/docs/reports/finance-trading/synthesis.md` — the final ranked xvision proposal

If the wiki build emits HTML pages, the rendered output should mirror the same structure and keep the same directory grouping.

---

## Implementation Units

### U1. Clean the xvision baseline and normalize the target list

**Goal:** start from a clean `xvision` state and turn the finance-trading category into a deduped, research-ready target list.

**Requirements:** baseline clean state, normalized repo list, anomaly handling, traceable source list.

**Dependencies:** none.

**Files:**
- `FOLLOWUPS.md`
- `crates/xvision-dashboard/wiki/research-index.md`
- `crates/xvision-dashboard/wiki/xvision.md`
- `docs/superpowers/plans/2026-06-03-001-feat-finance-trading-autoresearch-sweep.md`

**Approach:**
- reset or discard local noise before any new research output is written
- extract the finance-trading entries from the source category
- dedupe repeated repo names into one target each
- record the non-repo anomaly separately so the sweep stays repo-scoped

**Test scenarios:**
- The xvision working tree is clean before research output is created.
- The normalized target list matches the source category after deduping repeated repo names.
- The non-repo anomaly is recorded and not treated as a normal repo target.

**Verification:**
- The normalized target list is stable and can be reused by later units without ambiguity.
- The baseline state no longer contains uncommitted local noise that could contaminate the batch.

---

### U2. Run one xvision-focused DeepSeek research pass per repo target

**Goal:** generate a comparable research note for each repo target in the finance-trading category.

**Requirements:** per-repo research, xvision-focused lens, DeepSeek research mode, comparable output shape.

**Dependencies:** U1.

**Files:**
- `research-wiki/docs/reports/finance-trading/repos/<repo-slug>.md`
- `research-wiki/docs/reports/finance-trading/index.md`

**Approach:**
- use one prompt template for every repo target
- force every run to answer the same three xvision questions:
  - what strategy idea does this repo suggest?
  - what should xvision borrow for the Optimizer / Autoresearcher?
  - what should xvision borrow for the agent harness?
- keep the research source-grounded and focused on usable patterns, not generic praise
- write the result as a stable per-repo note so later synthesis can compare them directly

**Test scenarios:**
- Every unique repo target produces exactly one per-repo note.
- Every note includes xvision-reuse angles for strategy, optimizer, and harness.
- The output shape is consistent enough that the synthesis step can compare notes mechanically.

**Verification:**
- The batch has one note per repo target, and the notes are ready for ranking without re-reading source materials.

---

### U3. Publish the batch into the research wiki directory

**Goal:** make the research browseable inside the wiki instead of trapping it in ad hoc output files.

**Requirements:** dedicated directory, batch landing page, per-repo pages, discoverability.

**Dependencies:** U2.

**Files:**
- `research-wiki/docs/reports/finance-trading/index.md`
- `research-wiki/docs/reports/finance-trading/repos/<repo-slug>.md`
- `research-wiki/docs/reports/finance-trading/synthesis.md`
- `research-wiki/docs/research-index.md`
- `research-wiki/docs/xvision.md`
- `research-wiki/docs/xvision.html` or the wiki equivalent generated artifact, if the build produces HTML from markdown

**Approach:**
- add a landing page for the batch with the repo list and run notes
- keep each repo note in its own file under the same directory
- add links from the existing xvision research surfaces so the batch is easy to find later
- preserve the research as a dated bundle so future sweeps can be compared against it

**Test scenarios:**
- The finance-trading batch has a single discoverable landing page.
- Each repo note is reachable from the landing page.
- The batch is linked from the existing xvision research navigation.

**Verification:**
- A reader can start from the xvision research page and reach the new finance-trading batch without guessing filenames.

---

### U4. Synthesize the improvement proposal

**Goal:** collapse the batch into a single ranked proposal for xvision improvements.

**Requirements:** ranked value, token-estimated effort, one-sentence usefulness, collapse duplicates across sources.

**Dependencies:** U2, U3.

**Files:**
- `research-wiki/docs/reports/finance-trading/synthesis.md`
- `research-wiki/docs/reports/finance-trading/index.md`

**Approach:**
- group overlapping ideas into one xvision improvement when multiple repos point at the same pattern
- rank by expected value to xvision first, then by token-effort estimate
- use a compact output schema so the final proposal is easy to scan and compare
- keep one sentence per improvement explaining why it is useful to xvision

**Test scenarios:**
- The synthesis contains a ranked list of improvements.
- Every item has a value rating, a token-effort estimate, and one sentence of usefulness.
- Repeated themes across repos are collapsed instead of duplicated.
- The final list is clearly organized around xvision-relevant impact areas.

**Verification:**
- The synthesis reads like a decision aid, not a raw evidence dump.
- The ranking can be used to choose follow-up work without reopening the whole research batch.

---

### U5. Polish the batch for discoverability and follow-up use

**Goal:** make the output easy to revisit and easy to turn into later xvision work.

**Requirements:** clean navigation, clear naming, reusable batch structure.

**Dependencies:** U3, U4.

**Files:**
- `research-wiki/docs/research-index.md`
- `research-wiki/docs/xvision.md`
- `crates/xvision-dashboard/wiki/research-index.md`
- `crates/xvision-dashboard/wiki/xvision.md`
- `docs/superpowers/plans/2026-06-03-001-feat-finance-trading-autoresearch-sweep.md`

**Approach:**
- add or refresh the links that expose the new batch in the xvision research surfaces
- make the batch naming obvious enough that future sweeps can append beside it
- keep the final proposal separate from any future implementation plan so the research bundle stays reusable

**Test scenarios:**
- The new batch is discoverable from the main xvision research surfaces.
- The naming scheme makes the batch easy to find again later.
- The proposal stays separate from implementation work.

**Verification:**
- A future reader can find the batch, open the per-repo notes, and read the ranked proposal without external context.

---

## Risks & Dependencies

- The `100x` CLI / DeepSeek path may impose rate limits or auth failures during a large sweep.
- The source category may drift between planning and execution, so the normalized target list should be captured once and reused.
- The xvision working tree is already dirty, so the clean-baseline step needs to happen before new writes.
- The research wiki may require a build step or index update to expose new pages in navigation.
- The final synthesis can get noisy if the per-repo notes are not normalized to the same prompt shape.

---

## Success Metrics

- The finance-trading category is normalized into a stable target list.
- Every unique repo target has a matching research note.
- The research wiki has a dedicated finance-trading batch directory.
- The final synthesis ranks improvements by value and token effort.
- Every ranked item has a one-sentence explanation of why it helps xvision.
- The batch is easy to navigate from the existing xvision research surfaces.
