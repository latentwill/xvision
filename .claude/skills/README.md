# Skills under `.claude/skills/`

Two different kinds of skills live here — they share a directory because both
follow Anthropic's YAML-frontmatter markdown skill format, but the *consumers*
are different. Don't confuse them.

If your session is rooted in this repo, load the skill that matches the job:

- `.claude/skills/xvision-cli/SKILL.md` for operator tasks: using `xvn`,
  dashboard/remote-CLI operation, strategy/eval workflows, and run diagnosis.
  Its frontmatter name is `xvision`, so agent UIs may display it as
  `xvision` even though the directory is `xvision-cli/`.
- `.claude/skills/xvision-dev/SKILL.md` for repo contribution tasks:
  Rust/TS code, migrations, contracts, CI, deploy scripts, and architecture.
- `.claude/skills/xvision-cli-qa/SKILL.md` for API/CLI contract QA without
  relying on the browser UI.

Do not use old references to `.claude/skills/xvision/SKILL.md` for the main
operator skill; the active operator skill is `xvision-cli/SKILL.md`.

| Path | Consumer | Purpose |
|---|---|---|
| `xvision-cli/` | **Claude Code** (this CLI, the IDE extensions, agent SDK sessions) | Operator/usage skill: `xvn` CLI surface, dashboard/remote-CLI operation, strategy/eval workflows, provider readiness, launch diagnostics. |
| `xvision-cli-qa/` | Claude Code / agent SDK sessions | Raw API / HTTP contract QA for Strategy, Scenario, Eval, Provider, and diagnostics flows. |
| `xvision-dev/` | Claude Code / agent SDK sessions | Engineering skill for contributors editing code, migrations, CI/deploy, contracts, or architecture. |
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
$EDITOR .claude/skills/xvision-cli/SKILL.md
$EDITOR .claude/skills/xvision-dev/SKILL.md
$EDITOR .claude/skills/xvision-cli-qa/SKILL.md

# Submodule catalogs — refresh, then commit the pin:
git submodule update --remote .claude/skills/byreal
git submodule update --remote .claude/skills/mantle
```

Pin submodules to a tagged release before any submission run.
