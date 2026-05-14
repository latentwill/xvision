# ADR 0012 — Deprecate the Plan 2b in-app skills surface

**Date:** 2026-05-12
**Status:** Accepted
**Supersedes:** Plan 2b (`docs/superpowers/plans/2026-05-08-strategy-engine-2b-skills.md`)
**Related:** `docs/superpowers/plans/2026-05-11-agents-page-v1.md` (PR #83)

## Scope

This ADR deprecates **only** the Plan 2b surface — the `xvision-skills`
crate, the `xvn skill {new,ls,attach}` CLI, and the three MCP verbs
(`xvn_create_skill`, `xvn_list_skills`, `xvn_attach_skill_to_agent`).

It does **not** deprecate the agents-page-v1 plan's *workspace skill
registry* concept (`docs/superpowers/plans/2026-05-11-agents-page-v1.md`
§Skills). That concept — registry entries of `kind = tool |
prompt_fragment | evaluator` with optional `config_schema`, multi-selected
per slot via `AgentSlot.skill_ids[]` — is a distinct abstraction that
shares only the *name*. The `skill_ids` field on `AgentSlot` and the
`agent_slots.skill_ids_json` migration column are retained as the
forward-compat hook for that v1.1 registry.

## Context

Plan 2b shipped on 2026-05-08 as the `xvision-skills` crate, the `xvn
skill {new,ls,attach}` CLI subcommand, and three MCP verbs. A *Plan 2b
skill* was an OSShip-style markdown file (YAML frontmatter + body) saved
at `$XVN_HOME/skills/<name>.md`. `attach_skill_to_agent` mutated a
`StrategyBundle` slot in-place: it overwrote `slot.prompt`, set
`model_requirement`, and unioned `allowed_tools`.

Three days later (2026-05-11, PR #83) the Agents page shipped per
`docs/superpowers/plans/2026-05-11-agents-page-v1.md`. It introduced a
new first-class entity:

```
Agent { name, description, tags, slots: AgentSlot[] }
AgentSlot { name, provider, model, system_prompt, skill_ids[], max_tokens }
```

`AgentSlot` owns the prompt, provider/model, and token budget directly.
Plan 2b's "attach a markdown file to overwrite the slot prompt" loop is
exactly what a free-text `system_prompt` field on `AgentSlot` already
does — but now scoped to the canonical `Agent` entity rather than the
outgoing `StrategyBundle`.

Two surfaces, same job, different entities:

| Plan 2b skill responsibility | AgentSlot equivalent |
|---|---|
| Override slot prompt body | `system_prompt` (free-text textarea in `SlotForm`) |
| Set model requirement | `model` + provider picker |
| Union allowed tools | (not in v1 of Agents; if/when added, lives on `AgentSlot`) |
| Store a named, reusable prompt | `Agent` is itself the reusable, named, library-listed unit |

Plan 2b's `xvn skill attach` operated on `StrategyBundle` slots — the
outgoing per-strategy slot model that the Agents page is replacing as
the "intern as a top-level concept" scaffolding from the xianvec era.
Maintaining the surface means maintaining an authoring path that points
at a deprecated entity shape.

Secondary motivation: namespace collision. The Plan 2b "skill" name
collided with both Claude Code skills (`.claude/skills/`) and the
distinct agents-plan skill registry concept. The xvision skill in
`.claude/skills/xvision/SKILL.md` had to carry a paragraph
disambiguating its name from the Plan 2b concept — a smell that the
namespace was going to keep biting authors and agents.

## What the agents-plan "skill" is (and why it stays)

`docs/superpowers/plans/2026-05-11-agents-page-v1.md` §Skills defines a
different abstraction:

```
A skill in v1 is a named entry in a workspace-level skill registry.
- skill_id (ULID)
- name, description
- kind: tool | prompt_fragment | evaluator
- config_schema (optional JSON schema for skill-specific config)
```

These three kinds do not overlap with existing `AgentSlot` fields:

- **`tool`** — MCP-style callable. `AgentSlot` does not currently expose
  `allowed_tools` in v1, so this is not redundant with anything in the
  slot.
- **`prompt_fragment`** — composed (prepended/appended) onto the slot's
  `system_prompt` at runtime. Plan 2b *overwrote* the prompt; a fragment
  is the "compose multiple snippets into one slot" use case, which
  `system_prompt` (a single textarea) does not support.
- **`evaluator`** — post-decision check. No slot-field equivalent.

The picker is hidden in v1 because no registry exists yet. The plan
explicitly says: "If `/settings/skills` doesn't exist by ship time, the
picker is hidden entirely and skills come back in v1.1." `skill_ids[]`
on `AgentSlot` and the `agent_slots.skill_ids_json` column are the
forward-compat hook for that registry — keeping the field means
existing agents survive the registry landing without a schema migration.

## Decision

1. **Remove the Plan 2b surface in full:**
   - Delete the `xvision-skills` crate and its workspace registration.
   - Delete the `xvn skill` CLI subcommand (`commands::skill`, the
     `Skill` variant on `Command`, and the two `tests/{skill_cli,
     exit_codes_skill}.rs` files).
   - Delete the three MCP skill verbs from
     `crates/xvision-mcp/src/tools.rs` along with their request shapes,
     the `skill_store()` helper, and the test block under `mod tests`
     that exercised them.

2. **Retain the agents-plan forward hook:**
   - Keep `AgentSlot.skill_ids: Vec<String>` (Rust) and `skill_ids:
     string[]` (TS).
   - Keep the `agent_slots.skill_ids_json` migration column.
   - Keep the SlotForm's empty-state-aware skill indicator, the
     AgentList "Skills" column, and the `skill_ids: vec![]` /
     `skill_ids: []` template + test seeds.

3. **Documentation:**
   - Mark Plan 2b superseded at the top of its file.
   - Rewrite `MANUAL.md`, `crates/xvision-engine/README.md`,
     `v1-shipping-plan.md`, and `.claude/skills/xvision/{SKILL.md,
     references/{cli,architecture}.md}` to drop the Plan 2b surface
     and point at this ADR.
   - Annotate `AgentSlot.skill_ids` (Rust + TS) and the SQL column with
     "forward-compat hook for the v1.1 skill registry, distinct from
     Plan 2b" so future contributors don't confuse the two.

## Alternatives considered

**Rename the Plan 2b CLI to `xvn prompt-pack` (or `xvn slot-template`)
and keep the surface.** Preserves the technical debt: every reason to
rename — namespace collision, conceptual duplication with
`AgentSlot.system_prompt` — applies just as strongly to a renamed CLI.
We would still be carrying two authoring paths for the same job, one of
which points at the deprecated `StrategyBundle` entity. Net: more code
to maintain, more docs to disambiguate, no new capability.

**Re-target `attach_skill_to_agent` at `Agent` instead of
`StrategyBundle`.** Closer to viable but still leaves two ways to set
the same fields (markdown file on disk vs. form in UI). The Agent form
is already the canonical surface for `system_prompt`, `provider`,
`model`, and `max_tokens`; adding a parallel "attach a markdown file"
path means maintaining merge semantics, version mismatch between
file-state and DB-state, etc. Not worth it for the v1 user base.

**Also delete the agents-plan `skill_ids` field.** Considered briefly
and rejected. The agents-plan skill registry is a different
abstraction — `tool` and `evaluator` kinds have no existing slot-field
equivalent, and `prompt_fragment` (composable) is meaningfully different
from `system_prompt` (single textarea). Removing the field would force
a schema migration when the registry lands in v1.1, and would dispose
of design work that is *not* what this ADR is rejecting.

## Consequences

- Marketplace-distributed reusable prompts (Plan 5, blockchain) publish
  `Agent`s against the on-chain registry, not Plan 2b's
  `StrategyBundle`-bound markdown files. The publishable unit is the
  Agent (or, eventually, a registered skill from the v1.1 registry).
- The "Skill chip" / "skill picker modal" entries in the design handoff
  cuts in `v1-shipping-plan.md` are partially redirected: the chip /
  picker still ships when the v1.1 registry lands, but now reads from
  the registry rather than from Plan 2b's filesystem store.
- The CLI exit-code documentation no longer references the deleted
  Plan 2b skill error paths (codes 4 and 7 examples).
