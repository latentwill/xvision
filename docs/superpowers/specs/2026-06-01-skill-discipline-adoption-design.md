# Skill discipline adoption — design

> Status: locked 2026-06-01
> Scope: `.claude/skills/xvision/` (the three xvision operator skills)
> Companion: `docs/superpowers/plans/2026-06-01-skill-discipline-implementation.md`
> Affects: autoresearch-ops, flywheel-ops, memory-ops SKILL.md files + fixture test harness

## 1. Problem

The three xvision skills (autoresearch-ops, flywheel-ops, memory-ops) lack the
structural sections needed to make them reliable router targets. An agent
reading a skill file has no guidance on when NOT to use it, no example trigger
prompts, and no documented gotchas. The result:

- Skill invocation is inconsistent: related tasks land in the wrong skill or no
  skill at all.
- Ambiguous prompts (e.g. "activate \<id\>") have no documented resolution.
- No test harness validates that the right skill is chosen for a given prompt.
- New operators cannot calibrate which skill to invoke from the file alone.

## 2. Body-hardening standard

Every xvision skill MUST contain these sections, in this order relative to the
existing body:

### Required sections (insertion order)

Insert after the intro paragraph, before the first operational section:

1. `## When to use` — 2–4 sentences, affirmative use case only.
2. `## When NOT to use` — explicit exclusions, each pointing at the correct skill.
3. `## Trigger examples` — 5–8 concrete prompt strings that SHOULD invoke this skill.

Insert after the last operational/invariants section, before the end:

4. `## Gotchas` — 3–6 labeled bullets describing common operator mistakes.
5. `## Owner` — track name and contract file reference.

### Conventions

- `## When NOT to use` bullets: `task description → use \`<skill-name>\``
- `## Trigger examples`: raw prompt strings (no code blocks), one per `- ` line.
- `## Gotchas`: bold label, then description: `**Label**: correction.`
- `## Owner`: `<track> (\`team/contracts/<track>.md\`)`

## 3. Router benchmark format

Fixtures live at `.claude/skills/xvision/skill-fixtures.json`. Each entry:

```json
{
  "id": "<kebab-slug>",
  "prompt": "<natural-language prompt the agent receives>",
  "expected_skill": "<name: field from SKILL.md frontmatter>",
  "should_not_trigger": ["<name>", "..."],
  "notes": "<disambiguation rationale>"
}
```

Field rules:
- `id`: required, kebab-case, unique across all fixtures.
- `prompt`: required, a realistic operator or agent utterance.
- `expected_skill`: required, must exactly match the `name:` frontmatter field of
  the target SKILL.md (e.g. `xvision-autoresearch-ops`, not the directory name).
- `should_not_trigger`: optional list of skills that MUST NOT be chosen.
- `notes`: optional, documents why this case exists.

### Coverage requirements per skill

| Requirement | Minimum |
|---|---|
| Positive fixtures (prompt → this skill) | 3 |
| Ambiguity fixtures (prompt resolvable to exactly one skill) | 1 |

## 4. Validation script

`scripts/skill-router-benchmark.sh` (optional path override as first arg).

Behaviour:

1. Discovers all SKILL.md files under `.claude/skills/xvision/` and builds a
   registry of `name:` frontmatter values.
2. Parses the fixture JSON (hard cap: MAX_FIXTURES=100 per NASA Power-of-10 §2).
3. Per fixture: validates required fields; verifies `expected_skill` is in the registry.
4. Prints `PASS`/`FAIL` per fixture with a final count line.
5. Exits non-zero on any failure.

The script does NOT run prompts through a live LLM. It validates structural
correctness of fixtures (named skill exists, required fields present). Manual
routing-accuracy review is the skill author's responsibility.

## 5. Skill priority list

| Skill | Path | Priority | Rationale |
|---|---|---|---|
| `xvision-autoresearch-ops` | `.claude/skills/xvision/autoresearch-ops/SKILL.md` | P0 | Most operator-facing; terminology lock dependency |
| `xvision-memory-ops` | `.claude/skills/xvision/memory-ops/SKILL.md` | P0 | F+L+T safety — gotchas are highest risk |
| `xvision-flywheel-ops` | `.claude/skills/xvision/flywheel-ops/SKILL.md` | P1 | Narrower scope; audit/release path |

All three skills ship in the same PR as this spec so no intermediate state exists
where the standard is defined but the skills don't conform to it.

## 6. Out of scope

- LLM-in-the-loop routing accuracy testing (no automated prompt → skill routing exists).
- Skills outside `.claude/skills/xvision/` (superpowers plugin skills are a separate system).
- Adding new skills (this spec is hardening, not expansion).
- Changing operational content (commands, invariants, evidence sections) in the
  skills — only structural sections are added.
