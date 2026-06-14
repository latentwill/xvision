# Live-trading terminology lock — 2026-06-13

> Status: locked 2026-06-13 (CT5 live-deployments contract)
> Scope: live-run API surface (`/api/live/deployments*`), `LiveDeploymentSummary`,
> `LiveRunState`, and the dashboard live-deployments page.
> Companion: `docs/superpowers/specs/2026-06-13-ct5-live-deployments-contract-design.md`
> Does NOT cover: the autooptimizer subsurface (see
> `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`)
> or any DSPy/optimizer engine tokens.

## The two-surface principle

Every named concept has a developer-surface name (Rust types, SQLite
columns, spec docs, API fields) and an operator-surface name (CLI help,
UI labels, dashboard text). Both are locked here.

## Mapping conventions

| Symbol | Meaning |
|---|---|
| `dev` | Developer-surface name (code, spec, schema) |
| `ops` | Operator-surface name (CLI, UI, ops docs) |
| — | No rename; term is the same on both surfaces |

## Section 1 — Live-trading core concepts

| dev | ops |
|---|---|
| `deployment` / `LiveDeploymentSummary.deployment_id` | "Deployment" — one live eval run (paper or testnet). Never "run" in the operator UI. |
| `realized_pnl_usd` + `unrealized_pnl_usd` | "Running P&L" — displayed as a combined or split figure per the strip plan; always labelled "P&L", never "profit". |
| `deployed_capital_usd` | "Deployed capital" — the initial capital committed to this deployment. Shown as a USD figure in the strip. |
| `daily_loss_remaining_usd` / `daily_loss_limit_remaining_usd` | "Daily-loss buffer" — how much loss headroom remains before the daily-loss limit triggers. Never "drawdown limit", "stop-loss balance", or "remaining limit". |
| `venue_label = "paper"` or `"testnet"` | "Simulated" — the operator-surface label for paper and testnet runs. Never "paper trading", "demo", or "sandbox" in UI copy; use "Simulated". (The `venue_label` wire value stays `"paper"` / `"testnet"` in the JSON.) |

## Section 2 — Internal dev-only tokens (no operator exposure)

These terms appear only in Rust source and schema; they are never shown to operators.

| dev token | Notes |
|---|---|
| `live_run_state` | SQLite table name; per-run capital-risk snapshot written by `run_inner_live` each bar. |
| `LiveStateStore` | Rust store struct; wraps the upsert/get interface to `live_run_state`. |
| `mode = 'live'` | `eval_runs.mode` column value that gates which rows appear in the deployments API. |
| `risk_veto_count` | Count of risk-gate vetoes for this run; displayed as "Risk vetoes" in the strip (follow-on plan). |
| `drawdown_pct` | Computed peak-to-trough drawdown fraction; displayed as "Max drawdown" (follow-on strip plan). |

## Amendments

None yet.
