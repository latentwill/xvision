# Agent Access and CLI Discoverability — Spec

**Date:** 2026-05-12  
**Surfaces:** Root GitHub README, agent skill packaging/docs, `xvn` CLI discoverability, tailscale remote execution  
**Status:** Draft for user review

## Goal

Make xvision operable by external and embedded agents without hidden knowledge.

This pass is about agent usability and safe access. An agent should be able to discover how to use the repo, how to install or locate the xvision skill, which `xvn` commands exist, and how to reach the live node over the current tailscale-only remote-control path.

## Scope

This spec covers:

- agent-facing repo guidance in the root GitHub `README.md`
- skill discoverability and usage examples
- CLI discoverability and surface completeness checks
- tailscale-only remote execution guidance and alignment with the existing remote CLI design

This spec does **not** cover the UI/state consistency fixes from the QA surface pass. Those belong to the companion QA Pass 4 surface-consistency spec.

## Product Rules

1. Agent workflows must be documented in the repo itself, not hidden in local-only tooling assumptions.
2. The documented agent path must match the actual shipped `xvn` CLI surface.
3. For now, remote agent control targets the Tailscale-served xvision node only.
4. Broader remote-host support and stronger connection security are follow-up work, not implicit v1 promises.

## Problem Summary

The current repo assumes too much unstated context:

- the agent skill is not discoverable enough
- the skill coverage/examples are too narrow for the real CLI surface
- parts of the docs still imply certain CLI commands are deferred or unavailable when code paths already exist
- the repo does not give a GitHub-facing overview telling an agent how to orient itself and which path to use
- the operator intent around remote execution has shifted from ad hoc SSH toward a safer tailscale-served control plane, but the documentation is not yet consolidated around that choice

## Batch 1 — Root README Agent Overview

### Goal

Add a GitHub-facing operator/agent overview to the root `README.md` so an agent landing on the repo can understand how to use xvision without relying on hidden local context.

### Required changes

Add a dedicated section in the root `README.md` that covers:

- what an agent should read first in this repo
- where the xvision-specific skill lives and how to install or use it
- where the canonical CLI help and examples live
- how to use the dashboard vs the CLI
- how to reach a live node for remote execution in the current tailscale-only model

### Required content

The section should point agents to at least:

- `MANUAL.md`
- `FOLLOWUPS.md`
- the relevant `docs/superpowers/specs/` and `docs/superpowers/plans/`
- the xvision skill documentation/path
- the canonical CLI surface/help path

### Acceptance criteria

- A new agent-oriented overview exists in the root `README.md`.
- The overview is written for GitHub readers, not only local contributors.
- The section tells an agent where to find or install the xvision skill and where to find command examples.

## Batch 2 — Skill Discoverability and Coverage

### Goal

Make the xvision skill easy to find and useful across the actual `xvn` workflow surface.

### Required changes

- Document the skill in the repo in a place an agent will actually read.
- Ensure the skill coverage reflects the real CLI/dashboard vocabulary: strategies, agents, evals, scenarios, providers, dashboard, and remote execution expectations.
- Add good concrete examples, not only abstract descriptions.

### Example classes the skill/docs should cover

- create/list/show strategies
- run or inspect evals
- browse scenarios
- configure providers
- use the dashboard when the task is UI-facing
- use the tailscale-exposed remote control path when acting against a live node

### Acceptance criteria

- An agent can discover the skill path and usage from repo docs.
- The skill/examples cover more than just the narrow authoring path.
- The examples reflect the actual modern command names and flows.

## Batch 3 — CLI Discoverability and Surface Audit

### Goal

Remove drift between what the repo says the CLI can do and what the CLI actually exposes.

### Required changes

- Audit the top-level CLI docs, help text, README copy, and agent docs against the real `xvn` subcommands.
- Fix stale statements that claim a command is deferred when it already exists.
- Identify any genuinely missing command surfaces that the product expects agents to use and either:
  - expose them, or
  - document that they are intentionally unavailable.

### Specific issue to address

The current codebase already exposes `xvn eval run`, but some docs/help text still describe that path as deferred follow-up work. This mismatch must be removed.

### Acceptance criteria

- Repo docs do not falsely describe already-shipped CLI commands as unavailable.
- The documented CLI surface matches the current implementation closely enough that an agent can trust it.
- Any intentionally unexposed command class is called out explicitly rather than implied.

## Batch 4 — Tailscale-Only Remote Agent Access

### Goal

Define the current remote agent access story clearly and consistently: use the Tailscale-served node, not arbitrary remote hosts.

### Decision

For this pass, xvision supports **tailscale-only** remote control of the current node. The target is the node already serving the dashboard over Tailscale, not a generic SSH orchestration model and not arbitrary host routing.

This work should align with the existing spec:

- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`

### Required changes

- Update agent-facing docs so the preferred remote-control path is the Tailscale-served dashboard/remote CLI surface.
- Avoid implying that agents should rely on ad hoc shell or arbitrary SSH access for standard operations.
- Make clear that remote execution is constrained to typed `xvn` operations, not a general shell.

### Acceptance criteria

- Agent docs consistently describe the tailscale-only remote-control path.
- The repo does not present arbitrary SSH access as the primary long-term control model.

## Batch 5 — Follow-up Note on Connection and Security

### Goal

Record the deferred connection/security work explicitly so the current tailscale-only decision does not get mistaken for the final architecture.

### Required follow-up note

The docs/spec should include a follow-up section that mentions:

- the earlier need/operator habit of telling agents to use an SSH connection to the server
- why the current pass intentionally narrows scope to the Tailscale-served node instead
- that broader remote-host support remains future work
- that future connection/security work may include auth, capability tokens, rate limiting, or a different remote-control transport

### Acceptance criteria

- The follow-up note explicitly mentions the SSH-originated requirement and its narrowed tailscale-only replacement.
- The note makes it clear that broader connection/security design is not finished in this pass.

## Documentation hierarchy

The agent path in repo docs should have a clear hierarchy:

1. **Root README** for orientation
2. **MANUAL / xvision skill docs** for operational behavior
3. **CLI help and examples** for exact command usage
4. **Remote CLI over Tailscale spec** for the live-node remote execution contract

## Out of scope

- UI cleanup on dashboard surfaces
- Strategy persistence/refresh bugs
- Full auth design for remote execution
- General multi-host orchestration
- Replacing the CLI entirely with MCP-over-HTTP in this pass

## Risks

1. If the README overview and the skill docs diverge, agents will still choose the wrong entry point.
2. If stale CLI docs remain in even one high-visibility place, agents will distrust the repo and fall back to guessing.
3. If the tailscale-only scope is not stated plainly, users will assume arbitrary remote SSH support is part of the shipped design.

## Result

When this pass is complete:

- an agent can land on the GitHub repo and orient itself quickly,
- the xvision skill is discoverable and useful,
- the documented CLI surface matches reality,
- and the remote-control story is clear: use the current Tailscale-served node now, with broader connection/security work tracked explicitly as follow-up.
