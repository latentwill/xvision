# marketing/demos/

Scripts for live demonstrations — podcast appearances, AMAs, hackathon
showcases. Each demo is a markdown file with a setup checklist, a
beat-by-beat script, and a fallback for when the live path breaks.

Treat these as **rehearsable**, not ad-libbed. The operator runs through any
demo at least twice in a clean environment before showing it externally.

## Demo inventory

| Demo | File | Where it's shown | Status |
|---|---|---|---|
| Live overnight run | `live-overnight-run.md` | Tier-1 podcast demos, Tier-1 AMAs | ⚪ deferred — needs the Optimizer loop live |
| Build-from-scratch strategy | `build-from-scratch.md` | Onboarding-leaning podcasts, dev streams | ⚪ deferred |
| Risk-gate veto walk-through | `risk-veto-walkthrough.md` | Trust-narrative podcasts (Bankless, Unchained) | ⚪ deferred |
| Lineage tree explorer | `lineage-tree-explorer.md` | Identity / reputation podcasts (ERC-8004 working group, NFT shows) | ⚪ deferred |

⚪ = deferred · 🟡 = in development · ✅ = ready to show

## Demo script template

When creating a new demo file, follow this structure:

```markdown
# Demo — <name>

**Audience:** <who this lands with>
**Duration:** <target minutes, with hard cap>
**Pre-flight:** <checklist before going live>

## Beats

1. [00:00–00:30] Setup line — what the audience is about to see and why.
2. [00:30–02:00] First beat — concrete action, name what you're doing.
3. ...

## Fallback

If <thing that can fail> fails: <what we show instead>. Pre-rendered
video at `marketing/demos/fallbacks/<name>.mp4`.

## Verifiability hooks

- run_id shown on-screen: <yes/no>
- audit-log link mentioned: <yes/no>
- ERC-8004 attestation surfaced: <yes/no>

## Last rehearsed

- Date:
- Operator:
- Issues found:
```

## Demo principles

1. **Show real artifacts.** Real `run_id`s, real `agent_id`s, real
   attestation hashes. Never staged data.
2. **Name the failure mode before it happens.** "If the overnight loop
   doesn't converge, we kill the parent and fall back to the last sealed
   variant — let me show you when that happened on this run."
3. **Always have a fallback.** Live demos break. Pre-rendered MP4 of the
   golden path is on disk, ready to play.
4. **Show the audit log.** Every demo ends with "and here's where you'd
   verify this yourself" → audit-log query or chain explorer link.
