# xvision — Team coordination

Filesystem-based coordination for parallel Claude / Codex sessions and
human contributors. The board, contracts, and ownership map all live as
plain git-tracked files; no external coordination tool is required.

## Start here

- **Workers:** read `MANIFEST.md` → `board.md` (or `board-v2.md` for V2 work)
  → your contract under `contracts/<track>.md` → the briefing template at
  `briefings/_template.md`.
- **Conductors:** read `CONDUCTOR.md`.
- **Reviewers:** every PR should link to its `contracts/<track>.md`. A PR
  whose diff falls outside its contract's `allowed_paths` is closed, not
  merged.

## Layout

```
team/
├── README.md           # this file
├── MANIFEST.md         # top-level pointers
├── board.md            # current-wave board
├── board-v2.md         # V2 roadmap board (V2A active, V2B+ roadmap)
├── CONDUCTOR.md        # conductor role + daily checklist
├── OWNERSHIP.md        # file-glob → owning track
├── CONFLICT_ZONES.md   # single-writer file registry
├── contracts/
│   ├── _template.md
│   └── <track>.md      # one per active track
├── briefings/
│   └── _template.md    # sync-before-work ritual
├── status/
│   └── <track>.md      # per-track current status (worker-owned)
├── queue/
│   └── <from>__<utc>__<topic>.md   # append-only messages
├── intake/
│   └── <date>-<wave>.md            # raw wave intake before decomposition
└── archive/
    └── <date>-<wave>/              # frozen state of closed-out waves
```

## CI lint

```bash
bash scripts/board-lint.sh
```

Run locally before opening a PR that adds or edits a contract.

## When a track lands

The conductor moves the contract from `team/contracts/` to
`team/archive/<date>-<wave>/contracts/`, marks any conflict-zone rows
released, and deletes the `task/<slug>` branch on origin the same day.

## Stale or unfamiliar files?

If something under `team/` doesn't match this README, it predates the
2026-05-16 overhaul and should be archived. Check
`team/archive/2026-05-16-migration/` first.
