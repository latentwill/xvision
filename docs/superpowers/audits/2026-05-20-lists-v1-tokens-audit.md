# Lists v1 — token audit

**Date:** 2026-05-20
**Auditor:** `list-component-tokens-reconcile` (phase 1c)
**Source A:** `docs/design/FilterSearchLists.zip` → `design_handoff_lists/styles.css`
**Source B:** `frontend/web/src/styles/tokens.css`
**Spec context:** `docs/superpowers/specs/2026-05-20-standard-list-component.md`
Decision 1 — token reconciliation is a near-no-op for folio-dark and
selective add-only for black/light.

## Method

1. Extract the `--*: value;` declarations from each `:root` /
   `[data-theme="*"]` block on both sides.
2. Diff token *names* — anything in the handoff not present in
   xvision is a required addition.
3. Diff token *values* for shared names — anything not visually
   equivalent is a delta and gets a verdict (`accept` /
   `fix-in-this-track` / `fix-in-phase-2`).
4. Visual-diff the lists components on each xvision theme against
   the handoff canvas (`Lists.html`, `Lists Mobile.html`) at the
   target viewport.

## Verdict

**Zero token-name delta** across all four xvision theme blocks
(`:root`, `[data-theme="folio-dark"]`, `[data-theme="black"]`,
`[data-theme="light"]`). Every token name the handoff defines is
already declared.

## Folio-dark (`:root` and `[data-theme="folio-dark"]`)

| Handoff token | Handoff value | xvision value | Delta |
|---|---|---|---|
| `--bg` | `#0F0E0C` | `#0f0e0c` | hex case (identical) |
| `--surface-sidebar` | `#17150F` | `#17150f` | hex case (identical) |
| `--surface-card` | `#14120E` | `#14120e` | hex case (identical) |
| `--surface-elev` | `#1B1810` | `#1b1810` | hex case (identical) |
| `--surface-panel` | `#221E14` | `#221e14` | hex case (identical) |
| `--surface-hover` | `#1F1C13` | `#1f1c13` | hex case (identical) |
| `--border` | `#2A2618` | `#2a2618` | hex case (identical) |
| `--border-strong` | `#3A3322` | `#3a3322` | hex case (identical) |
| `--border-soft` | `#221F15` | `#221f15` | hex case (identical) |
| `--text` | `#F1ECDD` | `#f1ecdd` | hex case (identical) |
| `--text-2` | `#A39A85` | `#a39a85` | hex case (identical) |
| `--text-3` | `#6B6553` | `#6b6553` | hex case (identical) |
| `--text-4` | `#4A4536` | `#4a4536` | hex case (identical) |
| `--gold` | `#D4A547` | `#d4a547` | hex case (identical) |
| `--gold-soft` | `#B8862E` | `#b8862e` | hex case (identical) |
| `--gold-bg` | `rgba(212,165,71,0.1)` | `rgba(212, 165, 71, 0.1)` | whitespace only (identical) |
| `--gold-bg-strong` | `rgba(212,165,71,0.18)` | `rgba(212, 165, 71, 0.18)` | whitespace only (identical) |
| `--warn` | `#DB9230` | `#db9230` | hex case (identical) |
| `--danger` | `#C8443A` | `#c8443a` | hex case (identical) |
| `--info` | `#6F8FB8` | `#6f8fb8` | hex case (identical) |
| `--radius-card` | `6px` | `6px` | identical |
| `--radius-sm` | `4px` | `4px` | identical |

**Verdict: accept.** Folio-dark is the handoff source of truth; the
existing port is byte-equivalent modulo hex case + interior
whitespace. No file edits required.

## Black theme (`[data-theme="black"]`)

The black theme is not in the handoff. It defines the same set of
token names with adjusted values for a pure-black surface:

- `--bg: #000` (vs folio-dark `#0f0e0c`)
- `--gold: #f0c75e` (lighter; more legible on pure black)
- `--surface-*` and `--border-*` are darker grayscale variants of
  the same role assignments.

**Visual-diff (Chrome at 390×844 and 1440×900):**

| Surface | Verdict | Notes |
|---|---|---|
| `<ListCard>` header (serif title + count pill) | accept | Cormorant Garamond renders crisp; count pill border at `#202020` is visible without being noisy. |
| `<ListToolbar>` search input | accept | `--surface-elev #101010` reads as a subtle inset; gold-soft focus border is bright on black. |
| `<ListToolbar>` filter pills (non-default state) | accept | `--gold/10` background reads through pure black; gold text legible. |
| `<ListActiveChips>` | accept | Same gold-tinted treatment as folio-dark; nothing to adjust. |
| `<MListCard>` body / row cards | accept | `--surface-card #080808` row cards on `#000` background give the right 1-step elevation. |
| `<MListSheet>` slide-up over content | accept | Backdrop `rgba(0,0,0,0.55)` is effectively invisible on black; the 2px blur and border-strong top edge still register the sheet's frame. |

**Verdict: accept.** No `tokens.css` edits; no follow-ups for black
theme.

## Light theme (`[data-theme="light"]`)

The light theme is not in the handoff. It defines the same token
names with light surfaces (`--bg: #f7f5ef`), dark text (`--text:
#201d18`), and a darker gold (`--gold: #8a5f16`) so accents survive
on parchment-toned surfaces.

**Visual-diff (Chrome at 390×844 and 1440×900):**

| Surface | Verdict | Notes |
|---|---|---|
| `<ListCard>` header | accept | Serif title renders dark on cream; count pill border at `#d8d0c2` is gentle. |
| `<ListToolbar>` search input | accept | Focus border switches to `--gold-soft #a87922`; high enough contrast. |
| `<ListToolbar>` filter pills (non-default state) | accept | `--gold/10` on a light surface is subtle — verifies the design intent that the gold tint is the *signal* not the *headline*. |
| `<ListActiveChips>` | accept | Underlined "Clear all" link readable. |
| `<MListCard>` body / row cards | accept | Card-on-bg differential is small (`#fffaf0` on `#f7f5ef`) but legible thanks to the 1px `--border #d8d0c2`. |
| `<MListSheet>` slide-up over content | accept | Backdrop `rgba(0,0,0,0.55)` correctly dims the light surface; sheet body reads as a paper card. |

**Verdict: accept.** No `tokens.css` edits; no follow-ups for light
theme.

## Follow-ups

None gating Phase 2. Two micro-polish items the audit surfaced but
deferred:

1. The `:root` and `[data-theme="folio-dark"]` blocks duplicate the
   same values. Collapsing them by making `[data-theme="folio-dark"]`
   inherit `:root` is a `tokens.css` cleanup that does not change
   behavior; out of scope for this contract.
2. The handoff CSS uses uppercase hex; xvision uses lowercase. A
   project-wide hex-case normalization is out of scope; leave as-is
   so cherry-picks from the handoff zip don't churn the diff.

## Closeout

Phase 2 list-migration contracts (`list-migrate-eval-runs`, etc.)
may proceed without a token-set rewrite. If a per-route theme tweak
surfaces during phase 2, file it as a follow-up; do not edit
`tokens.css` from a migration track.
