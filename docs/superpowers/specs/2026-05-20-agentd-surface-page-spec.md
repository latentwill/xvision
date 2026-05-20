# `xvision-agentd` Surface Page — Spec

**Date:** 2026-05-20
**Status:** Spec
**Track:** docs intake #12 (`docs-agentd-surface-page`) at
`team/intake/2026-05-20-docs-user-and-agent-wiki.md`
**Related:**
- Cline SDK sidecar design `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md`
- xvn Agent Run System `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md`
- Agent run observability UI `docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`

## Why this spec exists

The intake flagged #12 for a spec round-trip before a baked
documentation page is written. The reasoning: `xvision-agentd` is the
Node sidecar where the Cline SDK agent loop actually runs, but
**Rust is the system of record** for everything that needs to be
reproduced, audited, billed, or replayed. A page that describes "the
agentd surface" without first agreeing on the public/private split
will leak internal protocol surface into operator documentation and
calcify implementation details we still expect to churn.

This spec resolves that split. It is **not** the page itself. It is
the contract the page will adhere to once it lands.

## Audience for the page

Two readers, with different acceptance bars:

1. **An autonomous agent** (operator-authorized) writing a client that
   streams a single agent run end-to-end. Needs: connection sequence,
   the four methods it can actually call, the event stream it must
   consume, the shutdown contract.
2. **An operator** debugging a stuck run. Needs: where the socket lives,
   what `ready` on stderr means, how to read the protocol+sidecar+SDK
   version triple, when to suspect the sidecar vs the engine.

The page is **not** for SDK contributors — internal-only mechanics
(`active-run.ts`, `tool-shim.ts`, `mock-provider.ts`, budget timers,
the JSON-RPC dispatch internals) stay out of the page and remain
documented in code. Linking those readers to the source is fine; baking
their detail into a doc that ships with the operator binary is not.

## Architectural reminder (one paragraph for the page)

The page must open with this framing, in plain prose:

> `xvision-agentd` is the Node sidecar where the Cline SDK agent loop
> runs. The Rust workspace (`xvision-engine` + `xvision-agent-client`)
> owns the run ledger, scenario replay, credentials policy, tool
> implementations, observability, and every durable record of a run.
> The sidecar is replaceable infrastructure — it hosts the SDK,
> translates between Rust's canonical event shape and Cline's API, and
> routes tool calls back to Rust. Swapping it out should change only
> the sidecar.

Anything that contradicts this framing — e.g. "agentd persists
sessions", "agentd implements tools", "agentd holds credentials" — is
either out of date or a bug. The page should not document such
behavior even as an aside.

## Scoping decisions (lock these before writing the page)

These are the choices that need agreement before the page is written.
Each is stated as a decision, with a one-line rationale and the
in-page treatment.

| # | Decision | Rationale | In-page treatment |
|---|---|---|---|
| S1 | The page documents the **wire protocol** (JSON-RPC method names + result shapes + notification names), not the internal TS module layout. | The wire is the contract; the TS layout is implementation churn. | Method tables + event tables only; no module/file maps. |
| S2 | Method coverage is exactly the **four method families currently exposed**: `runtime.health`, `tool.registry.{set,get}`, `tool.invoke`, `session.{start_run,step,end_run}`. Anything not registered in `xvision-agentd/src/methods/index.ts` does not exist for the page. | Honest surface. The `mock-provider` test seam isn't part of the contract. | One table per family; an "Out of scope today" footer enumerates what's known-missing (cancellation RPC, multi-run multiplex, hot reload). |
| S3 | The page documents **notification names + their payload fields**, not the internal `emit.ts` helpers. | Notifications are what a client must parse; the helpers are an implementation aid. | One table with the 11 notification names (`event.run_started`, `event.run_finished`, `event.tool_call_*`, `event.model_call_*`, `event.assistant_text_delta`, `event.overloaded`, `event.error`) keyed by payload field list. |
| S4 | The page documents the **three transport endpoints** the sidecar uses: the request socket (`--socket`), the callback socket (`--callback-socket`, optional), and the event socket (`--event-socket`, optional). It does **not** document `xvision-agent-client`'s RPC fan-in/out side of those sockets. | The page is the agent-facing contract for talking *to* the sidecar; the Rust client is documented in its crate's rustdoc. | A small "Sockets" subsection lists role, framing (NDJSON-of-JSON-RPC), and ownership (who creates, who unlinks). |
| S5 | The page documents the **three-version handshake** (`protocol_version`, `sidecar_version`, `cline_sdk_version`) as the **only** way to identify what's deployed. Constants live in `xvision-agentd/src/version.ts`. | Both clients and operators need this. The protocol version is the only thing a client can branch on. | A short "Versioning" section spelling out which version is bumped on what kind of change. |
| S6 | The page documents the **`ready` line on stderr** as the operator-visible "alive" signal, and the **parent-PID liveness monitor** as the operator-visible "dies with its parent" signal. | These are the two operator-visible affordances. | A small "Lifecycle" section: spawn → ready line → handshake → method calls → SIGTERM/parent-loss. |
| S7 | The page documents **tool side-effect levels** (`pure`, `read_only`, `external_read`, `external_write`) and the **requires_approval** flag as the registry's safety surface. It does NOT document tool implementations themselves — those live in Rust and are documented by the MCP page (`/docs?slug=mcp`). | Tool surface duplication invites drift. | A one-paragraph footer that cross-links to the MCP page; the registry-set wire shape stays in the methods table. |
| S8 | The page documents **budget enforcement at the wire level only** (`budget_limits` field in `start_run`, `event.run_finished` with reason). Internals of `budget.ts` are out. | The wire is what's stable. | Mention `budget_limits` in the `session.start_run` row; note that exceedance produces `event.run_finished` with a `status` carrying the reason. |
| S9 | The page documents **redaction at the boundary** (input/output hashes on tool events, no raw payloads on the event stream) but does NOT enumerate the three-mode policy (`hash_only`/`redacted`/`full_debug`) — that's a Rust-side concern. | The sidecar emits hashes regardless; mode selection happens in the Rust client. | One sentence in the events section: "Tool inputs/outputs are SHA-256 over a stable JSON serialization; raw payloads stay on the request socket." |
| S10 | The page is **baked into the dashboard** at `docs/xvnwiki/agentd.md` with `section = "Agent"`, last_reviewed dated the day the page lands. | Same as every other wiki entry. | Register in `docs/xvnwiki/index.toml` after `mcp`, before `operator-manual`. |

## Concrete page outline

This is what the agentd.md file will contain once the spec is
accepted. Each section maps to one or more scoping decisions above.

1. **`# xvision-agentd`** — one-paragraph architectural reminder (verbatim text in §"Architectural reminder" above). Cross-link to `/docs?slug=driving-xvn-as-an-agent` and `/docs?slug=mcp`. (S0 framing)
2. **Lifecycle** — spawn → `ready` JSON line on stderr → handshake → method calls → SIGTERM-or-parent-loss → cleanup. Cite the actual stderr line shape: `{"event":"ready","socket":"<path>"}`. (S6)
3. **Sockets** — table of three socket roles:
   | Socket | CLI flag | Direction | Framing | Creator |
   |---|---|---|---|---|
   | request | `--socket <path>` | client → sidecar requests; sidecar → client responses | NDJSON-of-JSON-RPC 2.0 | client (caller passes path; sidecar `listen()`s) |
   | callback | `--callback-socket <path>` | sidecar → Rust tool dispatch | NDJSON | client |
   | event | `--event-socket <path>` | sidecar → Rust event sink | NDJSON-of-JSON-RPC notifications | client |
   (S4)
4. **Versioning** — three-version triple, what each version means, when each bumps. Mention `runtime.health` as the read path. (S5)
5. **Methods** — one subsection per method family with a table:
   | Method | Params (shape) | Result (shape) | Notes |
   |---|---|---|---|
   Cover `runtime.health`, `tool.registry.{set,get}`, `tool.invoke`, `session.{start_run,step,end_run}`. Use the actual Rust `protocol.rs` field names as the canonical wire shape (they round-trip through serde to TS). (S2, S7, S8)
6. **Events (notifications)** — table of the 11 names from `NOTIFY` in `xvision-agentd/src/session/emit.ts` with payload fields. Note the hash-only posture for tool inputs/outputs. (S3, S9)
7. **Out of scope today** — bullet list of known-missing pieces (no cancellation RPC, no multi-run multiplex, no hot tool reload — registry replacement requires session end + restart). (S2)
8. **Debugging recipes** — short troubleshooting list:
   - "I see EADDRINUSE on startup" → prior crash left the socket; sidecar unlinks best-effort, but a stale file pre-spawn needs manual `rm`.
   - "`ready` line never appears" → suspect `--socket` path permissions or `XVISION_TEST_MOCK_PROVIDER` env set in prod.
   - "Sidecar exits silently mid-run" → parent-PID liveness monitor; check whether the Rust supervisor died first.
   - "Version triple disagrees with what I expected" → image pulled the wrong artifact; check the deploy stack's compose pin.
9. **What `xvision-agent-client` (Rust) does on the other side** — one paragraph + a "see also" link to the crate's rustdoc. Explicit: the page is the wire contract; the Rust client is one of the wire's clients. (S4)

## Out of scope (for this spec and the page)

- **Cline SDK semantics** — agent loops, sub-agents, skills, snapshots. The page assumes the reader knows what an SDK agent does. Cline's own docs are the source.
- **Tool implementations** — they live in Rust, and the MCP page documents the externally-exposed subset.
- **Observability schema** — SQLite span table layouts, OTel attribute lists. Owned by the observability spec.
- **Run replay** — owned by the engine. The sidecar has no replay path; it produces events that Rust ledgers.
- **Credentials policy** — Rust-only. The sidecar receives a per-run `api_key` in `session.start_run` and never persists it.
- **Multi-run multiplex** — the sidecar today serves one run at a time per connection; multi-run is a future track.

## Acceptance for the page (not this spec)

- File lives at `docs/xvnwiki/agentd.md`, registered in
  `docs/xvnwiki/index.toml` after `mcp` and before `operator-manual`,
  under `section = "Agent"`, with `last_reviewed` set to the day it
  lands.
- Page starts with `# xvision-agentd`, body > 100 chars, passes the
  baked-content tests in `crates/xvision-dashboard/src/routes/docs/mod.rs`.
- Every method name, notification name, CLI flag, env var, and stderr
  line shape cited on the page is verifiable by grep against
  `xvision-agentd/src/` at the commit the page lands on.
- Page does **not** mention: `active-run.ts`, `tool-shim.ts`,
  `mock-provider.ts`, `budget.ts`, JSON-RPC error code constants by
  numeric value, or any internal-only test seam.
- A working agent could read the page top-to-bottom and write a client
  that drives a single run start → step → end with one tool call,
  consuming the event stream — without reading the TypeScript source.

## Acceptance for this spec

- Scoping decisions S1–S10 are accepted (or amended) by a reviewer
  with context on both Rust and Node sides.
- After acceptance, intake track #12 is unblocked. The implementation
  work is a single PR adding one markdown file + one manifest entry +
  a date bump in `index.toml`.

## Decisions deferred

- **Whether to also bake a TS client reference example** alongside the
  wire spec. Proposed: no — the page should be language-agnostic. If
  operators want a starter, they can read the `xvision-agent-client`
  Rust crate's tests for an executable reference. Re-open if the
  agent-driving feedback says the wire-only page is too sparse.
- **Whether to document the `XVISION_TEST_MOCK_PROVIDER=1` env var**.
  Proposed: yes, but only in the "Debugging recipes" section as the
  "what does this mean if I see it set in prod" item. It is a test
  seam, not a public knob.
- **Whether to surface the `tool.registry.set` `registry_hash` field**.
  Proposed: yes — it's a stability signal for clients that re-register
  the same tool set across reconnects. Mention in the methods table.

## Why this is P2, not P1

The page only matters to a small set of clients (the Rust
`xvision-agent-client`, possibly a future second-language client, and
operators debugging a stuck run). The default agent-driving path is
the `xvn` CLI, which is already documented at
`/docs?slug=driving-xvn-as-an-agent`. The sidecar is hidden behind
that surface unless something breaks. P2 is the right priority — the
absence of the page costs a future second-client author a half-day of
source-reading; it does not block any current operator workflow.
