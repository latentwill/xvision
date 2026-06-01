# Eval Review Agent

> **Status:** Execution-board intake · 2026-05-15
> **Related code:** `crates/xvision-engine/src/eval/`, `crates/xvision-engine/src/api/eval.rs`, `crates/xvision-dashboard/src/routes/eval_runs.rs`, `crates/xvision-cli/src/commands/eval.rs`, `frontend/web/src/routes/eval-runs-detail.tsx`

## Goal

After an eval run completes, Xvision can generate a structured analytical review
using a user-selected review agent. The review explains what happened, whether
the strategy appears viable, where it failed, what risks were detected, and what
should be tested next.

This is a review layer over completed eval artifacts, not a replacement for the
existing per-run metrics or lightweight findings extractor.

## Current Fit

The codebase already has:

- `eval_runs` with status, mode, metrics, token usage, and errors.
- `eval_decisions` with per-decision action, conviction, justification, fills,
  fees, and realized PnL.
- `eval_equity_samples` for run-detail and comparison charts.
- `eval_findings` as first-class rows, currently shaped for lightweight
  extractor output: `kind`, `severity`, `summary`, `evidence_json`.
- `/api/eval/runs`, `/api/eval/runs/:id`, `/api/eval/runs/:id/stream`, and
  `/api/eval/runs/compare/chart`.
- Dashboard routes at `/eval-runs` and `/eval-runs/:id`.
- CLI command ownership in `crates/xvision-cli/src/commands/eval.rs`.

The review feature should therefore add a new `eval_reviews` parent artifact and
extend or version the findings model so review findings remain first-class
objects linked to a review.

## Data Model

Add `eval_reviews`:

```sql
CREATE TABLE eval_reviews (
    id TEXT PRIMARY KEY,
    eval_run_id TEXT NOT NULL,
    agent_profile_id TEXT NOT NULL,
    status TEXT NOT NULL,
    verdict TEXT,
    confidence REAL,
    score INTEGER,
    summary TEXT,
    raw_output_json TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (eval_run_id) REFERENCES eval_runs(id)
);
```

Agent profiles should use the existing agent/provider/model concepts where
possible. If no suitable table exists for review-specific profiles, add a small
`agent_profiles` table seeded with:

- `fast-trader-agent`
- `reasoning-agent`
- `risk-agent`
- `research-agent`

Each profile stores `name`, `type`, `provider`, `model`, `temperature`,
`max_tokens`, `system_prompt`, and `enabled`.

Findings must stay first-class. Preferred migration path:

- Add review-oriented columns to `eval_findings`: `eval_review_id`, `type`,
  `confidence`, `title`, `description`, `recommendation`, `created_at`.
- Keep existing `run_id`, `kind`, `severity`, `summary`, `evidence_json`,
  `extracted_at`, and `schema_version` working for current extractor callers.
- Map review `type` to legacy `kind` and review `title/description` to legacy
  `summary` for compatibility until the frontend/API moves fully to the v2
  shape.

Review finding shape:

```json
{
  "id": "uuid-or-ulid",
  "eval_review_id": "uuid-or-ulid",
  "eval_run_id": "uuid-or-ulid",
  "type": "performance | risk | regime | behavior | execution | data_quality | anomaly | opportunity",
  "severity": "low | medium | high | critical",
  "confidence": 0.0,
  "title": "string",
  "description": "string",
  "evidence": [
    {
      "kind": "metric | trade | time_range | chart_region | event | log",
      "reference": "string",
      "value": {}
    }
  ],
  "recommendation": "string"
}
```

## Review Input Payload

The review service should build a bounded payload from persisted run artifacts:

```json
{
  "eval_run_id": "id",
  "agent_id": "strategy-or-agent-id",
  "scenario_id": "id",
  "mode": "backtest | paper",
  "status": "completed",
  "metrics": {},
  "equity_curve": [],
  "decisions": [],
  "events": [],
  "errors": [],
  "agent_profile": {
    "id": "reasoning-agent",
    "model": "configured-model",
    "temperature": 0.2,
    "max_tokens": 8000
  }
}
```

Do not invent unavailable artifacts. If orders, positions, market metadata, or
logs are not persisted yet, either omit them or include an empty array plus a
clear limitation in the prompt.

## Review Output Contract

The model must return strict JSON. Narrative text is allowed only inside JSON
fields.

```json
{
  "eval_review_id": "id",
  "eval_run_id": "id",
  "agent_profile_id": "reasoning-agent",
  "summary": "string",
  "verdict": "promising | weak | failed | inconclusive",
  "confidence": 0.0,
  "score": 0,
  "findings": [],
  "risks": [],
  "next_tests": [],
  "questions": []
}
```

Validation requirements:

- Require `summary`, `verdict`, `confidence`, `score`, `findings`, `risks`,
  `next_tests`, and `questions`.
- Require 3 to 10 findings for completed runs unless the eval payload is too
  sparse; if sparse, the review must be `inconclusive`.
- Require 1 to 5 risks and 3 to 7 next tests.
- Reject findings whose evidence references are not present in the payload.
- Preserve raw model JSON on `eval_reviews.raw_output_json` for audit.
- Persist normalized finding rows separately from the raw output.

## Agent Profiles

`fast-trader-agent`:

- Quick tactical read.
- Short, decisive, practical.
- Optimized for rapid iteration and obvious pass/fail issues.

`reasoning-agent`:

- Deeper causal analysis.
- Evidence-backed and explicit about uncertainty.
- Optimized for why a strategy worked or failed.

`risk-agent`:

- Downside, failure mode, drawdown, tail-risk, exposure, and sizing review.
- Should be strict about robustness and overfitting.

`research-agent`:

- Next experiment generation.
- Proposes scenario expansion, mutations, and hypothesis tests.

## Analysis Categories

Every review prompt should ask for:

- Performance: total return, Sharpe/Sortino where present, max drawdown, win
  rate, profit factor where present, expectancy where present, exposure, trade
  frequency.
- Equity curve quality: smoothness, drawdown clustering, recovery time, sudden
  jumps, late-run decay, dependence on one trade or window.
- Strategy behavior: entry/exit quality, holding time, churn, missed
  opportunities, repeated bad decisions, inconsistent signal behavior.
- Regime sensitivity: volatility, trend versus chop, liquidity, volume, and
  correlation shifts where the payload supports them.
- Risk: tail risk, drawdown concentration, leverage/exposure, position sizing,
  stop behavior, and failure modes.
- Data/execution quality: missing data, strange candles, unrealistic fills,
  slippage assumptions, paper/live mismatch, and scenario limitations.
- Viability: whether further research is justified, robustness versus
  fragility, strongest evidence, biggest concern, and next tests.

## API

Add:

- `POST /api/eval/runs/:id/review`
- `GET /api/eval/runs/:id/reviews`
- `GET /api/eval/reviews/:id`

Request:

```json
{
  "agent_profile_id": "reasoning-agent",
  "force": false
}
```

Response:

```json
{
  "eval_review_id": "id",
  "status": "queued | running | completed | failed"
}
```

For MVP, synchronous generation is acceptable behind the same endpoint if the
response still exposes status and persisted review id. Prefer a queued job if it
fits existing dashboard job infrastructure cleanly.

## CLI

Add:

```bash
xvn eval review <run_id> --agent reasoning
xvn eval review <run_id> --agent risk
xvn eval review <run_id> --agent research
xvn eval review <run_id> --agent fast-trader
xvn eval review <run_id> --agent reasoning --force
xvn eval review <run_id> --agent reasoning --output review.json
```

The default human output should print verdict, confidence, summary, key
findings, risks, and next tests. JSON output should match the API object.

## UI

On `/eval-runs/:id`, add a Review panel:

- Header with review status, selected agent, verdict badge, confidence,
  generated timestamp, and regenerate button.
- Agent picker for Fast Trader, Reasoning, Risk, Research, and later custom
  profiles.
- Sections for Executive Summary, Verdict, Key Findings, Risks, Evidence,
  Recommended Next Tests, and Open Questions.
- Finding cards showing severity, type, confidence, title, description,
  evidence links, and recommendation.

Use the existing run-detail route and API client. Do not create a parallel
`/eval/runs/:id` frontend path.

## MVP Acceptance

V1 is complete when:

- A completed eval run can be reviewed by a selected agent.
- The review persists to `eval_reviews`.
- Review findings persist as first-class finding rows linked to the review.
- `/eval-runs/:id` displays summary, verdict, findings, risks, and next tests.
- The user can regenerate a review with another agent or force-regenerate the
  same profile.
- CLI can generate and print/export a review.
- Output is deterministic enough to compare runs: low temperature, stable
  prompt, stable payload ordering.
- The review agent does not hallucinate metrics or trades not present in the
  payload.

## Deferrals

Do not include:

- Full autooptimizer loop.
- Automatic strategy mutation.
- Blockchain identity.
- Marketplace publishing.
- Live trading decisions.
- Smart contract settlement.
- Multi-agent debate.
- Long-term memory graph.
