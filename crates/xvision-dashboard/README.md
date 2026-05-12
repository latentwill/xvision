# xvision-dashboard

Embedded HTTP dashboard — axum routes serving the Vite-built React SPA in
`frontend/web/`. Boot with `xvn dashboard serve`; default bind is
`127.0.0.1:8788`. The frontend bundle is baked into the binary at compile
time via `static/` (populated by `npm run build` in `frontend/web/`).

```sh
xvn dashboard serve --bind 127.0.0.1:8788 --home ~/.xvn
```

## Command Palette (⌘K)

Press `⌘K` (or `Ctrl+K` on Linux/Win) anywhere in the app to open a
debounced fuzzy-search modal over every artifact in xvn:

| Group       | Source                                                |
|-------------|-------------------------------------------------------|
| Actions     | Static list seeded at startup (new strategy, settings, …) |
| Strategies  | `~/.xvn/bundles/<id>.json` via `StrategyStore::list`    |
| Runs        | `eval_runs` table via `RunStore::list`                |
| Findings    | `eval_findings` rows (per run) via `read_findings`    |
| Scenarios   | `canonical_scenarios()` (compiled-in fixed set)       |

Backend: `GET /api/search?q=&kind=&limit=` returns `{hits: SearchHit[]}`,
sorted by FTS5 BM25 then `updated_at desc`. Empty `q` returns the
most-recently-touched artifacts so the modal renders something useful on
first open. `kind=<single>` filters; `limit` is hard-capped at 200
server-side.

Indexing strategy:
- Cold start: `serve()` calls `engine::api::search::reindex_all` once,
  which walks the bundle store + run table and reseeds scenarios +
  actions. Idempotent — safe to re-run.
- Incremental: `api::strategy::{create_strategy,update_slot,set_risk_config}`
  and `api::eval::run_inner` upsert their artifact's row right after the
  underlying mutation succeeds. Best-effort: a failed index write logs at
  `warn!` and never breaks the calling write path.

Out of scope for v1 (see `docs/superpowers/plans/2026-05-10-command-palette-plan.md`):
- Personalized ranking — v1.1
- Body-content full-text (prompt bodies, finding evidence) — v1.1
- Customizable shortcut binding — never

## Routes

The full route table lives in `src/server.rs::build_router`. Highlights:

- `GET /api/health` — server + DB + bundle dir probes
- `GET /api/strategies`, `GET /api/strategy/:id` (+ inspector mutations)
- `GET /api/eval/runs`, `GET /api/eval/runs/:id`, `GET /api/eval/compare`
- `GET /api/search` — command palette
- `POST /api/wizard/chat` — wizard SSE
- `POST /api/chat-rail/*` — persistent chat rail sessions

All handlers are thin wrappers over `xvision_engine::api::*` and translate
`ApiError` → `DashboardError` → typed JSON via the impls in
`src/error.rs`.
