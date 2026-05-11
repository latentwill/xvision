# marketing/voices/

11labs voice persona configs. Each persona is a `.json` file capturing the
voice ID, model, stability / similarity / style settings, and the
pronunciation lexicon (especially important for financial / crypto jargon
that 11labs gets wrong by default).

## Persona inventory

| Persona | File | Use case | Status |
|---|---|---|---|
| `narrator-neutral` | `narrator-neutral.json` | Run-summary, compare, lineage videos. Calm, low-affect, technical. | 🟡 planned (Phase 0) |
| `founder` | `founder.json` | Concept-explainer videos. Cloned operator voice (consent required). | ⚪ deferred |
| `narrator-warm` | `narrator-warm.json` | Onboarding / Setup-walkthrough videos. Friendlier than `neutral`. | ⚪ deferred |

🟡 = next to build · ⚪ = deferred · ✅ = shipping

## Persona file shape (TBD on first build)

```jsonc
{
  "voice_id": "<11labs voice id>",
  "model": "eleven_turbo_v2_5",
  "settings": {
    "stability": 0.55,
    "similarity_boost": 0.75,
    "style": 0.15,
    "use_speaker_boost": true
  },
  "lexicon": {
    // Pronunciation overrides for terms 11labs gets wrong.
    // The voice synth treats these as the canonical pronunciation.
    "xvision": "ex vision",
    "Mantle": "mantle",
    "Orderly": "orderly",
    "ERC-8004": "E R C eighty oh oh four",
    "ULID": "you lid",
    "bps": "B P S",
    "PnL": "P and L",
    "Sharpe": "sharp"
  },
  "notes": "When this voice is right; when it isn't."
}
```

## When to use which voice

| Content type | Persona |
|---|---|
| Data-driven (run summary, compare, lineage) | `narrator-neutral` |
| Concept explanation (why-non-custodial, what-erc-8004-does) | `founder` (if available) or `narrator-warm` |
| Tutorial / onboarding (Setup walkthrough, mobile inspector) | `narrator-warm` |
| Risk warnings / safety / incident response | `narrator-neutral` (no warmth — clinical) |

## Consent for cloned voices

Founder voice clone requires written consent from the operator + an explicit
revocation policy ("if the founder leaves the project, the clone is retired
within 7 days; existing rendered videos remain"). Lives in
`decisions/voice-clone-consent.md` (create when first relevant).

## Cost notes

- 11labs Creator tier: ~$0.30 per minute of generated audio
- Cloned voices count against the same quota
- Voice library voices have no per-render rate beyond the standard quota
- Tracking: log per-video voice-API cost in the video's metadata (template-side)
