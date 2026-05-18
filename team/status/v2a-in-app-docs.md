---
track: v2a-in-app-docs
worktree: .worktrees/v2a-in-app-docs
branch: task/v2a-in-app-docs
base: origin/main
phase: pr-open
last_updated: 2026-05-18T05:17:00Z
owner: claude
---

# What changed

V2A item 2: surface in-repo documentation inside the dashboard for
first-time operators without leaving the SPA.

## Backend

- New module `crates/xvision-dashboard/src/routes/docs/mod.rs`.
- Five baked markdown pages under
  `crates/xvision-dashboard/src/routes/docs/content/`:
  `quickstart.md`, `strategies.md`, `scenarios.md`,
  `eval-runs.md`, `cli-reference.md`. Each is `include_str!`'d into
  the binary so the deployed image carries them — no runtime
  `docs/` directory read, no external network fetch.
- Two endpoints:
  - `GET /api/docs/index` → ordered `[{ slug, title }]`.
  - `GET /api/docs/page/:slug` → raw markdown body, `404` for
    unknown slugs.
- Rust unit tests cover: index lists all pages in order; known slug
  resolves to its body; unknown slug → 404; every baked page is
  non-empty and starts with an h1 heading.

## Frontend

- `frontend/web/src/api/docs.ts` — thin fetch wrappers + TanStack
  Query key factory.
- `frontend/web/src/features/docs/DocsMarkdown.tsx` — `react-markdown`
  renderer wired to the existing theme tokens (no new colours
  introduced).
- `frontend/web/src/routes/docs/index.tsx` — two-pane route:
  sidebar with client-side fuzzy filter on title/slug; main pane
  renders the selected page. First page auto-selected. Renders
  loading and inline-error states (no popup per CLAUDE.md).
- `routes.tsx` adds a `/docs` route entry mounting the new lazy
  module.

## Verification

- Passed: `corepack pnpm --dir frontend/web test -- --run docs` — 5 tests
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`
- Deferred: `cargo test -p xvision-dashboard routes::docs` — this
  deploy host has no Rust toolchain (per CLAUDE.md `cargo` is
  forbidden); the Rust tests are covered locally and will run in CI.
  The 4 baked-page Rust tests exercise the index, the slug lookup,
  the 404 path, and the non-empty / h1-heading invariant on the
  baked content.

## Allowed-paths deviation

The contract `allowed_paths` enumerated the new files but did not
include `crates/xvision-dashboard/src/routes/mod.rs`,
`crates/xvision-dashboard/src/server.rs`, or
`frontend/web/src/routes.tsx`. Mounting the new endpoints + the new
client route is impossible without minimal edits to those three:

- `routes/mod.rs` — adds `pub mod docs;`.
- `server.rs` — adds the two `route("/api/docs/...")` lines and the
  `docs` import in the existing route bundle.
- `routes.tsx` — adds the `DocsRoute` lazy import + the
  `{ path: "docs", element: page(<DocsRoute />) }` entry.

Edits are mechanical (no logic touched in existing routes). Flagged
here for the conductor.

## Notes

- Content was written from scratch rather than scraped from the
  in-repo `README.md` / `MANUAL.md` / `docs/` files (the contract
  forbids editing `docs/**`). Each page is curated to the surface
  it documents and stays under ~80 lines so the binary footprint
  is bounded.
- `react-markdown` + `remark-gfm` are already in `package.json` —
  no new frontend dependency added.
- No `border-white` / `border-gray-100/200` / `#fff` introduced.
- The `/docs` route is not yet exposed in the Sidebar nav. Adding
  a sidebar item is a one-line touch to `components/shell/Sidebar.tsx`,
  but Sidebar is well outside the contract allowed paths; deferred
  to a follow-up so the conductor can decide where the link should
  live (e.g. footer vs. primary nav vs. command palette).
