# Skills under `.claude/skills/`

Two different kinds of skills live here — they share a directory because both
follow Anthropic's YAML-frontmatter markdown skill format, but the *consumers*
are different. Don't confuse them.

If your session is rooted in this repo, Claude Code auto-discovers
`.claude/skills/xvision/`; that is the first xvision-specific skill an
external coding agent should load.

| Path | Consumer | Purpose |
|---|---|---|
| `xvision/` | **Claude Code** (this CLI, the IDE extensions, agent SDK sessions) — discovered automatically when a session is rooted in this repo | Orient a fresh Claude session in the xvision codebase: `xvn` CLI surface, pipeline vocabulary, deploy / build gotchas. F25 in `FOLLOWUPS.md`. |
| `xvision/memory-ops/` | Claude Code / agent SDK sessions | Operator runbook for persistent memory inspection, recall preview, Pattern lifecycle, and leakage probes. |
| `xvision/autoresearch-ops/` | Claude Code / agent SDK sessions | Operator runbook for offline Observation-to-Pattern distillation, numeric gates, blind Findings, and promotion/demotion evidence. |
| `xvision/flywheel-ops/` | Claude Code / agent SDK sessions | Operator runbook for flywheel status, velocity, lineage, memory-demo optimizer discipline, and release evidence. |
| `byreal/` (submodule) | **xvision's own Stage-1 Intern** runtime LLM (`crates/xvision-intern/`) — loaded as prompt context, not by Claude Code | Perpetual-futures domain knowledge (risk shapes, trade lifecycle). Satisfies hackathon Path 1 endorsement of Byreal Agent Skills via context loading even though execution venue is Orderly on Mantle. Upstream: [`byreal-git/byreal-agent-skills`](https://github.com/byreal-git/byreal-agent-skills). |
| `mantle/` (submodule) | **xvision's own Stage-1 Intern** runtime LLM | Mantle on-chain workflow (Mantle-touching setups, Phase 6.5 ERC-8004 work, Phase 11.5 Orderly forward run). Upstream: [`mantle-xyz/mantle-skills`](https://github.com/mantle-xyz/mantle-skills). |

`xvision/` is plain content checked into this repo. `byreal/` and `mantle/`
are git submodules pinned to upstream tags.

## Updating

```bash
# Claude Code skill — edit directly, PR like any other file:
$EDITOR .claude/skills/xvision/SKILL.md

# Submodule catalogs — refresh, then commit the pin:
git submodule update --remote .claude/skills/byreal
git submodule update --remote .claude/skills/mantle
```

Pin submodules to a tagged release before any submission run.
