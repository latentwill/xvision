# xvision-skills

OSShip-style markdown skills for xvn. Authors write a single markdown
file with YAML frontmatter; the runtime parses it, persists it under
`$XVN_HOME/skills/<name>.md`, and lets it be attached to a slot of any
saved `StrategyBundle`.

See spec: `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md` §6.

## What ships in v0.2 (Plan 2b)

- `Skill` type + `parse(markdown) → Skill` (YAML frontmatter + body, sha256 over the raw bytes)
- `SkillStore` trait + `FilesystemSkillStore` (`$XVN_HOME/skills/<name>.md`, raw markdown so the body roundtrips byte-exact)
- `attach_skill_to_agent(bundle, slot_role, skill)` — replaces slot prompt + model_requirement, unions allowed_tools
- 3 MCP verbs on `XvisionTools` (`xvn_create_skill`, `xvn_list_skills`, `xvn_attach_skill_to_agent`)
- CLI: `xvn skill {new | ls | attach}`

## What does NOT ship

- Marketplace publish / browse / install / attest — deferred to Plan 5 (blockchain integration)
- Skill versioning / lockfiles
- Skill discovery / registry beyond local filesystem
- Author identity / signatures (folded into Plan 5 with on-chain identity)

## Skill file format

```markdown
---
name: crypto-trader-base
display_name: "Generalist crypto trader"
description: "Default trader prompt for any crypto strategy"
version: 1.0.0
allowed_tools:
  - ohlcv
  - indicator_panel
model_requirement: "anthropic.claude-sonnet-4.6+"
---

You are a crypto trader. ...

Decide ONE of: long_open | short_open | flat | hold.
```

Required frontmatter fields: `name`, `display_name`, `description`,
`version`, `model_requirement`. `allowed_tools` is optional and
defaults to an empty list.

## CLI quick-start

```bash
# register / overwrite
xvn skill new --from-file my-trader.md
# → my-trader

# list
xvn skill ls

# attach to a slot of a saved strategy bundle
xvn skill attach 01H8N7ZAB... --slot trader --skill my-trader
# → attached my-trader → 01H8N7ZAB...#trader
```

## MCP

Same surface, exposed over MCP for AI agents:

- `xvn_create_skill { markdown } → { name, content_hash }`
- `xvn_list_skills {} → [{ name, display_name, description, version }]`
- `xvn_attach_skill_to_agent { agent_id, slot, skill_name } → { agent_id, slot, skill_name }`
