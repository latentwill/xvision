---
track: docs-agentd-surface-page
lane: leaf
wave: docs-user-and-agent-wiki-2026-05-20
worktree: .worktrees/docs-agentd-surface-page
branch: task/docs-agentd-surface-page
base: origin/main
status: ready                                                     # may need a spec round-trip first per intake §"Agent-facing documentation"
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/wiki/agentd.md                       # NEW — the page content
  - crates/xvision-dashboard/wiki/index.toml                      # add [[page]] entry for agentd
  - docs/superpowers/specs/2026-05-2x-agentd-surface.md           # OPTIONAL — spec round-trip if reviewer says the surface still needs design clarity first
forbidden_paths:
  - xvision-agentd/**                                             # do not modify the daemon — this is docs-only
  - crates/xvision-dashboard/src/routes/docs/**                   # the wiki plumbing is set; only add a page
  - crates/xvision-dashboard/wiki/*.md                            # all other wiki pages — different content scopes
  - frontend/web/**                                               # frontend renders the page; no frontend changes needed
  - crates/**                                                     # except the wiki manifest in dashboard
interfaces_used:
  - xvision-agentd UDS server                                     # the actual surface being documented (read-only reference)
  - NDJSON event schema                                           # documented from the daemon's source of truth
  - tool-shim registry                                            # documented as-is
verification:
  - test -f crates/xvision-dashboard/wiki/agentd.md
  - grep -q "agentd" crates/xvision-dashboard/wiki/index.toml
  - cargo build -p xvision-dashboard                              # ensures the build.rs picks up the new page
acceptance:
  - **New baked page at `crates/xvision-dashboard/wiki/agentd.md`.** Content covers, at minimum: (a) what `xvision-agentd` is (the TypeScript UDS server bundled with the binary), (b) UDS socket location and how to connect, (c) the NDJSON event schema for each event class (`session-start`, `session-step`, `tool-invoke`, `session-store`), (d) the tool-shim registry — what tools are exposed, what each one wraps, the auth posture, (e) session lifecycle from start to end, (f) the runtime health probe + how to read it.
  - **Acceptance bar from intake §`docs-agentd-surface-page`**: *"an agent reading only this page can write a client that streams a session end-to-end."* Worker validates by drafting a small smoke-test client (in test code or a doc-comment example) that exercises the documented surface.
  - **Index entry added.** `crates/xvision-dashboard/wiki/index.toml` gains a new `[[page]]` block with `slug = "agentd"`, an appropriate `title`, `section = "Agent"` (alongside the existing `driving-xvn-as-an-agent` page), `last_reviewed = <today>`, and `path = "agentd.md"`. Insert it in sidebar order adjacent to `driving-xvn-as-an-agent` so the agent-facing pages cluster together.
  - **Source of truth is the daemon.** Document only behavior visible in `xvision-agentd/src/`. If a documented behavior is wrong (the daemon does X but docs say Y), the docs change is the fix; if the daemon's behavior is wrong, that's a separate track (escalate before changing the daemon).
  - **Spec round-trip is conditional.** Per intake §`docs-agentd-surface-page`: *"May need a spec under `docs/superpowers/specs/2026-05-2x-agentd-surface.md` first; flag for conductor."* If the worker finds the daemon's externally-observable surface is unstable or undocumented in source, write the spec first, get it approved, then write the wiki page. If the surface is stable enough, the wiki page is the only deliverable.
  - **Build passes.** `cargo build -p xvision-dashboard` succeeds — the `build.rs` picks up the new page via the manifest and the generated `PAGES` table compiles.
  - **No daemon changes.** `xvision-agentd/**` is in `forbidden_paths`. If the worker finds an actual bug while writing docs, file a separate contract.

---

# Scope

Track #12 of `team/intake/2026-05-20-docs-user-and-agent-wiki.md`. The
TypeScript daemon `xvision-agentd` (UDS server, tool-shim, NDJSON event
stream) ships alongside the dashboard binary but has no documentation
in `/docs`. Agents inspecting the build don't have a single page that
explains how to talk to the daemon end-to-end.

This contract adds that page to the existing wiki (the build.rs +
`wiki/index.toml` plumbing was shipped 2026-05-20; just drop a new
`.md` and register it).

# Out of scope

- Modifying the daemon. If the surface is wrong, file a separate contract.
- Adding new sections to the wiki sidebar grouping (already shipped).
- Writing the MCP-surface page (track #13, separate concern).
- Any frontend changes — the `/docs` renderer is mode-aware and will
  handle the new page automatically.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/docs-agentd-surface-page status
git -C .worktrees/docs-agentd-surface-page log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/docs-agentd-surface-page -b task/docs-agentd-surface-page origin/main
```

# Notes

Recon (2026-05-21) confirms:

- `xvision-agentd/` exists at the repo root with `src/`, `test/`,
  `package.json`, `tsconfig.json`, `vitest.config.ts`. TypeScript stack.
- The wiki has an `Agent` section (per `wiki/index.toml` `section = "Agent"`)
  containing `driving-xvn-as-an-agent.md`. The new page joins this section.
- `cli-non-surfaced.md` already exists in the wiki for the "deliberately
  excluded from xvn" inventory. Cross-link from the agentd page if any
  of the daemon's tool-shim entries duplicate-or-bypass that surface.

The `section = "Agent"` (singular) value is established. Don't rename
to "Agents" — it'll fork the section header and split the rendered
sidebar block.
