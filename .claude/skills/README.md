# Skill catalogs

Two upstream skill catalogs vendored as git submodules. Both feed Stage 1 Intern's prompt context (`crates/xianvec-intern/`) and the Mantle on-chain workflow.

| Path | Upstream | Used by |
|---|---|---|
| `byreal/` | [`byreal-git/byreal-agent-skills`](https://github.com/byreal-git/byreal-agent-skills) | Stage 1 Intern — perpetual-futures domain knowledge (risk shapes, trade lifecycle). Satisfies hackathon Path 1 endorsement of Byreal Agent Skills via context loading even though execution venue is Orderly on Mantle. |
| `mantle/` | [`mantle-xyz/mantle-skills`](https://github.com/mantle-xyz/mantle-skills) | Stage 1 Intern (Mantle-touching setups) + Phase 6.5 ERC-8004 work + Phase 11.5 Orderly forward run. |

## Updating

```bash
git submodule update --remote .claude/skills/byreal
git submodule update --remote .claude/skills/mantle
```

Pin to a tagged release before any submission run.
