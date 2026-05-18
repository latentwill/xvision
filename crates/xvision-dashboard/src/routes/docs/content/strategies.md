# Strategies

A `Strategy` is the immutable pipeline configuration that drives an
eval. It composes one or more `Agent`s and binds them to roles
(intern → trader → risk → executor by convention).

## Anatomy

- **Manifest** — display name, tags, decision cadence, mechanical
  parameters (asset, capital, …).
- **AgentRefs** — `{ agent_id, role }` references into the agent
  library. Agents are reusable; strategies reference them by id.
- **Pipeline** — how the agents wire together. Single-agent
  strategies use `{ kind: "single" }`; multi-stage pipelines use
  `{ kind: "sequential", edges: [...] }`.
- **Risk** — gate parameters (max position, stop loss, etc.).
- **Mechanical params** — non-agent parameters that the executor
  reads directly.

## Author flow

1. **`/strategies/new`** opens a blank custom strategy form with an
   optional template dropdown. Picking a template autofills the
   name + agent slots; you can then tune.
2. **Save** creates a draft on disk in `$XVN_HOME/strategies/<id>.json`.
3. **Attach agents** through the Inspector (Strategy detail page).
   Use the chat rail to compose an agent from scratch, or attach
   an existing one from the agent library.
4. **Validate** — the dashboard runs the same checks the CLI does
   (`xvn strategy validate`) and surfaces missing-agent / missing-
   provider / model-resolution drift inline.

## Templates

Templates are reference scaffolds, not enforcement. They live in
`crates/xvision-engine/src/agents/templates.rs` (intern / trader /
risk role labels). Strategies may rename or invent roles freely.

## CLI parity

Every dashboard action has a CLI verb under `xvn strategy`:

- `xvn strategy ls` — list saved strategies.
- `xvn strategy show <id>` — print a single strategy.
- `xvn strategy add-agent / remove-agent / set-pipeline` — mutate
  the agent composition.
- `xvn strategy run --scenario <scenario-id>` — run the strategy
  against a scenario without the dashboard.
