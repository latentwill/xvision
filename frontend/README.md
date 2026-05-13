# frontend

The xvision web UI. Two subdirectories:

- `web/` — **the production frontend.** Vite + React + Tailwind SPA, built into
  `crates/xvision-dashboard/static/` and embedded into the `xvn` binary at
  compile time via `rust-embed`. This is what `xvn dashboard serve` actually
  ships.
- `prototype/` — original Claude Design handoff mockup (HTML/JSX, six screens
  on a 1440×900 canvas). Retained as visual reference; not built, not shipped.

See `DESIGN.md` for the synthesis between the prototype's screens and the real
engine API surface.

## Building `web/`

```sh
cd frontend/web
pnpm install
pnpm dev          # local Vite dev server on http://localhost:5173 (proxies /api to dashboard)
pnpm build        # emits to crates/xvision-dashboard/static/, ready to bake into the binary
pnpm typecheck    # tsc -b
```

After `pnpm build`, rebuild the dashboard crate (`cargo build -p xvision-dashboard`
or `cargo build -p xvn`) so `rust-embed` picks up the new assets. `xvn dashboard
serve` binds to `127.0.0.1:8788` by default.

The `crates/xvision-dashboard/static/` directory is `.gitignore`d; CI / Docker
builds must run `pnpm build` before `cargo build`.

### Remote access (Tailscale, LAN)

The dev server (`pnpm dev`) binds `0.0.0.0:5180` and accepts Tailscale MagicDNS
names (`*.ts.net`) — see `vite.config.ts`. The production dashboard binary still
defaults to loopback; override with `xvn dashboard serve --bind 0.0.0.0:8788`
(or narrower, the tailnet IP) to reach it from a phone or another device.

> ⚠ **Security caveat — no auth on the API.** The dashboard's trust model
> (DESIGN.md §8.4) assumes same-origin localhost. Binding wider than loopback
> exposes the engine API to anyone who can reach the interface. Safe behind a
> Tailscale ACL or on a trusted home network; **do not** expose `0.0.0.0` on a
> shared network (coffee shop, conference Wi-Fi, hotel) without adding auth.
> Tracked as a follow-up — see FOLLOWUPS F35.

## Routes shipping in v1

Implemented in `web/src/routes/`:

| Route | File | Purpose |
|---|---|---|
| `/` Dashboard | `home.tsx` | Workspace status, KPI tiles, recent runs |
| `/setup` | `setup.tsx` | Wizard (chat-driven onboarding + drafting) |
| `/strategies` | `strategies.tsx` | Drafts list, lineage column |
| `/authoring/:id` | `authoring.tsx` | Inspector: strategy tree + agent-role editor |
| `/eval-runs` | `eval-runs.tsx` | Leaderboard with filters + Compare select |
| `/eval-runs/:id` | `eval-runs-detail.tsx` | Run detail (equity, findings, ledger) |
| `/eval-runs/compare` | `eval-compare.tsx` | Side-by-side run comparison |
| `/settings/{providers,brokers,danger}` | `settings/*.tsx` | LLM/broker config |

Out of v1: marketplace, autoresearcher dashboard, journal/lab notebook,
`/live/<id>` flight deck. See `v1-shipping-plan.md` for the cut list.

## Viewing the prototype (reference only)

```sh
cd frontend/prototype
python3 -m http.server 8000
# open http://localhost:8000/
```

`index-print.html` is a print-friendly stack of all six artboards (Cmd+P →
Save as PDF, landscape, 1440×900). The prototype uses Babel-in-the-browser, no
build step. Treat it as visual source-of-truth, not as code to copy.

## Visual tokens (Folio dark)

Defined in `prototype/styles.css` and mirrored in `web/src/styles/`:

- **Surfaces** — warm-black ink (`#0F0E0C` bg → `#221E14` deepest panel)
- **Borders** — warm slate (`#2A2618`, `#3A3322`, `#221F15`)
- **Text** — `#F1ECDD` → `#4A4536` (4 levels)
- **Accent** — amber-gold `#D4A547` (with `--gold-soft`, `--gold-bg`)
- **Status** — warn `#DB9230`, danger `#C8443A`, info `#6F8FB8`
- **Type** — Cormorant Garamond italic (display), Inter (UI), JetBrains Mono (numerics, IDs)
- **Radii** — 6px cards, 4px small

## Domain mapping (prototype → xvision crates)

Per `CLAUDE.md` terminology (`cycle_id`, `agent_id`, `Strategy`):

- "Strategy" in prototype copy = `Strategy` in code
- Inspector "Layers" (Data / Regime / Intern / Trader / Entry-Exit / Risk / Execution) map to the pipeline crates: `xvision-data`, `xvision-intern`, `xvision-trader`, `xvision-risk`, `xvision-execution`
- "Run ID" (e.g. `01H8N7Z`) is a ULID; one run produces many `cycle_id`s (briefing → decision → outcome) — see `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`
- The Setup wizard's "Drafting" panel is the user-facing surface of the engine API foundation work in `docs/superpowers/plans/`
