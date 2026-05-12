# ADR 0012 — Deprecate the in-app skills surface in favor of Agents

**Date:** 2026-05-12
**Status:** Accepted
**Supersedes:** Plan 2b (`docs/superpowers/plans/2026-05-08-strategy-engine-2b-skills.md`)
**Related:** `docs/superpowers/plans/2026-05-11-agents-page-v1.md` (PR #83)

## Context

Plan 2b shipped on 2026-05-08 as the `xvision-skills` crate, the `xvn
skill {new,ls,attach}` CLI subcommand, and three MCP verbs
(`xvn_create_skill`, `xvn_list_skills`, `xvn_attach_skill_to_agent`).
A *skill* was an OSShip-style markdown file (YAML frontmatter + body)
saved at `$XVN_HOME/skills/<name>.md`. `attach_skill_to_agent` mutated a
`StrategyBundle` slot — overwriting `slot.prompt`, setting
`model_requirement`, and unioning `allowed_tools`.

Three days later (2026-05-11, PR #83) the Agents page shipped per
`docs/superpowers/plans/2026-05-11-agents-page-v1.md`. It introduced a
new first-class entity:

```
Agent { name, description, tags, slots: AgentSlot[] }
AgentSlot { name, provider, model, system_prompt, skill_ids[], max_tokens }
```

`AgentSlot` already owns the prompt, provider/model, and token budget
directly. The `skill_ids[]` field was a stub: the v1 plan documented
that the registry UI would live at `/settings/skills` and explicitly
flagged "if `/settings/skills` doesn't exist by ship time, the picker is
hidden entirely and skills come back in v1.1."

This left the codebase carrying two authoring surfaces for the same
job:

| Plan 2b skill responsibility | Agents page equivalent |
|---|---|
| Override slot prompt body | `AgentSlot.system_prompt` (free-text textarea in `SlotForm`) |
| Set model requirement | `AgentSlot.model` + provider picker |
| Union allowed tools | (not in v1 of Agents, but belongs on `AgentSlot` if/when added) |
| Store a named, reusable prompt | `Agent` is itself the reusable, named, library-listed unit |

The redundancy was structural, not cosmetic. The two surfaces also
targeted *different* entities — `xvn skill attach` operated on
`StrategyBundle` slots, the outgoing per-strategy slot model that the
Agents page is replacing as the "intern as a top-level concept"
scaffolding from the xianvec era. Maintaining both meant supporting an
authoring path that points at a deprecated entity shape.

A secondary motivation: the in-app concept's name collided with Claude
Code skills (the contents of `.claude/skills/`). The xvision skill in
`.claude/skills/xvision/SKILL.md` had to carry a paragraph disambiguating
its name from the in-app concept — a "smell" warning that the two
namespaces were going to keep biting authors and agents.

## Decision

Remove the in-app skills surface in a single PR, before anyone builds the
v1.1 skill registry on top of the stub. Re-center reusable prompt
authoring on the `Agent` entity.

Specifically:

1. Delete the `xvision-skills` crate and its workspace registration.
2. Delete the `xvn skill` CLI subcommand (`commands::skill`, the `Skill`
   variant on `Command`, the two `tests/{skill_cli,exit_codes_skill}.rs`
   files).
3. Delete the three MCP skill verbs from `crates/xvision-mcp/src/tools.rs`
   along with their request shapes, `skill_store()` helper, and the test
   block under `mod tests` that exercised them.
4. Drop `skill_ids` from `AgentSlot` — Rust struct, TS API type,
   `agent_slots.skill_ids_json` column, all `skill_ids: vec![]` / `[]`
   template + test seeds, the unused "Skills" column in the Agents list
   table, and the `/settings/skills` placeholder in `SlotForm`.
5. Rewrite the relevant docs (`MANUAL.md`, `.claude/skills/xvision/`,
   `v1-shipping-plan.md`) to drop the surface and point at this ADR.
6. Mark Plan 2b superseded at the top of its file.

## Alternatives considered

**Rename the CLI to `xvn prompt-pack` (or `xvn slot-template`) and
keep the surface.** Preserves the technical debt: every reason to
rename — namespace collision, conceptual duplication with Agents —
applies just as strongly to a renamed CLI. We would still be carrying
two authoring paths for the same job, one of which points at the wrong
entity. Net: more code to maintain, more docs to disambiguate, no new
capability.

**Re-target `attach_skill_to_agent` at `Agent` instead of
`StrategyBundle`.** Closer to viable but still leaves two ways to set
the same fields (file-on-disk vs. form-in-UI). The Agent form is already
the canonical surface for `system_prompt`, `provider`, `model`, and
`max_tokens`; adding a parallel "attach a markdown file" path means
maintaining merge semantics, version mismatch between file-state and
DB-state, etc. Not worth it for the v1 user base.

## Consequences

- The `skill_ids` field that was added to `agent_slots` in migration
  `005_agents.sql` is dropped *in place* in the same migration file
  rather than via a new migration. The agents feature only shipped
  three days ago and has no production data; editing 005 keeps the
  migration history clean.
- Marketplace-distributed reusable prompts (Plan 5, blockchain) return
  against the `Agent` entity, not `StrategyBundle`. The "publishable
  unit" becomes an Agent (or a named slot configuration), which is what
  the marketplace contract surface should reference anyway.
- The "Skill chip" / "skill picker modal" entries in the design handoff
  cuts in `v1-shipping-plan.md` are now moot. Design handoff descope:
  no chip, no picker.
- The CLI exit-code documentation no longer references skills (codes 4
  and 7 examples).
