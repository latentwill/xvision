# marketing/

Public-facing materials and the methods for producing them. Treated as a
durable asset library, not a scratchpad — every file here is something that
can be reused across pillars.

## Structure

```
marketing/
├── README.md              # this file
├── methods.md             # canonical methods doc — read first
├── pitch/                 # podcast + partner outreach copy
│   └── cold-email.md      # the one-paragraph pitch (Pillar 2)
├── templates/             # Remotion video templates
│   └── README.md          # template inventory + status
├── voices/                # 11labs persona configs
│   └── README.md          # voice inventory + when to use which
├── demos/                 # scripts for live demos (podcast appearances, AMAs)
│   └── README.md          # demo inventory
└── discord-pulse.md       # weekly running log of what target servers are debating
```

## How to use this folder

- **Producing a new run-summary video** → `templates/run-summary.tsx` + a `RunBundle` JSON
- **Reaching out to a podcast** → `pitch/cold-email.md` (template), customized per host
- **Updating the Discord engagement plan** → `discord-pulse.md` weekly
- **Adding a new voice persona** → `voices/<persona>.json` + entry in `voices/README.md`
- **Scripting a live demo for a podcast or AMA** → new file in `demos/`

## Voice + visual brand

Three non-negotiables, repeated from `methods.md` for visibility:

1. **Numbers are specific.** "Saved 3.2% on funding drag" beats "saved a lot."
2. **Risks are named.** Every output acknowledges what could go wrong.
3. **Verifiability is one link away.** Performance claims include a `run_id` + audit-log pointer + (when applicable) an ERC-8004 attestation hash.

Folio dark visual tokens (`#0F0E0C` warm-black, `#D4A547` amber-gold,
Cormorant Garamond display, Inter UI, JetBrains Mono numerics) carry through
every visual asset. Sourced from `frontend/web/src/styles/tokens.css`.

## What this folder is NOT

- A scratchpad for half-formed ideas (use `decisions/` or a personal note)
- A place for paid-marketing assets (we're not running paid ads — see `methods.md` § "Why these three")
- A copy of `docs/HACKATHON-1-PAGER.md` — that doc lives in `docs/` because it's primary product narrative; this folder distributes that narrative
