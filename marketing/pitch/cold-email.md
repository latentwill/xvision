# Podcast cold-email pitch

The canonical one-paragraph pitch. Customize the **bracketed** sections per
host; keep everything else as-is unless `methods.md` § "Topic threads to
develop" surfaces a better hook for a specific show.

---

## Template

```
Subject: AI trading agents that improve themselves overnight — pitch for <show>

Hi <host>,

I'm building xvision — non-custodial AI trading agents that run on Orderly +
Mantle, with an overnight autooptimizer that mutates strategies, evaluates
variants against held-out backtests, and seals survivors as ERC-8004 lineage
NFTs. There are three threads in here I'd love to talk through on <show>:

- Why "non-custodial" matters more than people think — what happens when the
  trading platform can place orders but can't withdraw, and why this changes
  the FTX failure mode at the substrate level.

- The autooptimizer: how an LLM judge avoids overfitting in a domain where
  every wrong call costs real money. (Spoiler: held-out windows, lineage
  attestation, kill-the-parent-if-the-child-wins.)

- ERC-8004 in production: portable reputation as the actual product, not the
  marketing. What happens when an agent's track record is on-chain.

I can demo a live overnight run if useful. Pre-recorded fallback available.

— <operator name>
```

## Customization checklist

Before sending, the email passes these checks:

- [ ] **Recent-episode reference.** Add one sentence between the greeting and the first paragraph that references a recent episode of the show. Specific, technical, not flattering. Example: "Loved the [date] episode on [topic] — your point about [specific] is part of why I think this is worth a conversation."
- [ ] **Thread reorder.** Move the most-relevant thread to position 1. For a DeFi-infrastructure show, lead with non-custodial. For an AI/ML show, lead with autooptimizer. For an NFTs/identity show, lead with ERC-8004.
- [ ] **Show-specific hook.** Append one sentence after the threads that connects to the show's framing. Example, for Bankless: "This is the bridge from 'trust the team' to 'trust the contracts' in agent-deployed capital."
- [ ] **One link, max.** Body links to one of: `docs/HACKATHON-1-PAGER.md` (if there's a public mirror), the project README, or a representative Remotion video. Never multiple. Calendly links go in the reply, never the cold email.
- [ ] **Length budget.** Whole email under 200 words. The pitch as written is ~175 words; the customization adds 25–50. Cut if over 225.

## Follow-up

If no reply in 8 business days, send one follow-up. One only. Subject prepended `Re:`. Body:

```
Hi <host>,

Following up on the pitch below in case it landed in the wrong inbox. No
worries if not a fit — happy to know either way.

— <operator name>

----- forwarded message -----
<original email>
```

If no reply in 8 more business days, the answer is no. Move on; do not send a
third follow-up.

## What we don't do

- No mass-bcc'd outreach
- No "we should grab coffee" framing
- No PDF attachments (one-pager content goes inline if needed)
- No tracking pixels
- No follow-up automation tools
