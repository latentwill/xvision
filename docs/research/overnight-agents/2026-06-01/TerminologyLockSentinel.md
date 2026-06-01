# TerminologyLockSentinel

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/` and follow-up slice `.100x/runs/20260601_003231/`
**Script:** scripts/terminology-lock-sentinel.sh

## Surfaces Inspected

Operator-facing surfaces only (developer-facing specs and Rust source identifiers are excluded):

- `MANUAL.md` — operator manual
- `crates/xvision-dashboard/wiki/**` — operator wiki / CLI reference
- `crates/xvision-cli/src/commands/autoresearch.rs` — CLI argument struct (becomes operator-visible flags)
- `frontend/web/src/features/marketplace/**` — marketplace UI
- `frontend/web/src/features/memory/**` — memory surface UI

Lock doc reference: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`

## Findings

### HIGH — CLI flag name violations (autoresearch.rs)

**File:** `crates/xvision-cli/src/commands/autoresearch.rs:120,123,126`

```rust
pub parent_holdout_score: Option<f64>,   // line 120 → --parent-holdout-score
pub child_holdout_score: Option<f64>,    // line 123 → --child-holdout-score
pub gate_epsilon: Option<f64>,           // line 126 → --gate-epsilon
```

These Rust field names serialize directly into clap CLI flags. They are operator-visible:

| Current flag | Required by lock doc |
|---|---|
| `--parent-holdout-score` | `--baseline-untouched-score` |
| `--child-holdout-score` | (no explicit rename specified; coordinate with lock doc) |
| `--gate-epsilon` | `--min-improvement` |

The lock doc states: *"Cryptographic primitives (BLAKE3, Ed25519, 'merkle,' canonical JSON) must never appear on an operator surface."* and lists the `--gate-epsilon` → `--min-improvement` pair.

### HIGH — Wiki CLI reference table

**File:** `crates/xvision-dashboard/wiki/cli-reference.md:216`

```
| memory-demos-gate ... --parent-holdout-score <n> --child-holdout-score <n> [--gate-epsilon <n>] ... |
```

The table row documents the wrong flag names and must be updated alongside the CLI rename.

### MEDIUM — Marketplace fixture data

**Files:**
- `frontend/web/src/features/marketplace/data/fixtures/listings.ts:107,108,116`
- `frontend/web/src/features/marketplace/data/fixtures/receipts.ts:8`

```typescript
manifestHash: "blake3:7f2b1ad91c4",       // line 107
operatorSig: "ed25519:7f2b1ad91c4",        // line 108
{ kind: "merkle", label: "Snapshot...",    // line 116
```

Cryptographic primitive names appear as fixture string values visible in the UI.

### MEDIUM — Marketplace type literal

**File:** `frontend/web/src/features/marketplace/data/types.ts:105`

```typescript
anchors: { kind: "merkle" | "mint" | "commit"; ... }[]
```

`"merkle"` as a TypeScript union literal flows through to UI render logic.

### MEDIUM — ReceiptsDrawer CSS map key

**File:** `frontend/web/src/features/marketplace/routes/ReceiptsDrawer.tsx:25`

```typescript
merkle: "text-info",
```

Used as a CSS class selector key; `merkle` appears in the rendered DOM as a data attribute or conditional class.

### MEDIUM — Test assertions on `merkle` text

**Files:**
- `frontend/web/src/features/marketplace/routes/ReceiptsDrawer.test.tsx:57`
- `frontend/web/src/features/marketplace/routes/TradeHistoryTable.test.tsx:97`

Tests assert `merkle` renders as visible text. Any rename of the type literal must co-update these tests.

### LOW — MANUAL.md dependency context

**File:** `MANUAL.md:235,237`

```
making `solana-sdk`/`ed25519-dalek 1.x` ...
`ed25519-dalek 2.x`, dropping the `zeroize = "=1.3.0"` exact pin.
```

This is dependency version context describing a technical upgrade path, not an operator-facing label or flag name. Borderline — likely acceptable in a developer changelog section. Flag for lock-doc owner review.

## Files Changed

- `scripts/terminology-lock-sentinel.sh` — reusable CI-safe scanner.
- `docs/research/overnight-agents/2026-06-01/TerminologyLockSentinel.md` — this finding.

## Verification

```bash
bash scripts/terminology-lock-sentinel.sh
```

Observed exit code: 1 (violations present as of 2026-06-01). Will exit 0 after remediation.

## Remediation Roadmap

1. **CLI flags (autoresearch.rs)**: rename struct fields and update clap long-name attributes. Coordinate with lock doc for the full mapping. Pre-launch breaking change — update docs, tests, and any shell scripts that call these flags.
2. **Wiki (cli-reference.md:216)**: update the table row to match the renamed flags.
3. **Marketplace type literal (types.ts:105)**: rename `"merkle"` to an operator-plain term. Coordinate with design on what label to use. Update all consuming sites in parallel (fixtures, ReceiptsDrawer, tests).
4. **Fixture strings (listings.ts, receipts.ts)**: replace `blake3:`, `ed25519:`, `merkle` strings with the plain-language equivalents from the lock doc.
5. **MANUAL.md**: review with lock-doc owner; likely acceptable as-is.

## Residual Risks

- Renaming the `"merkle"` type literal requires co-updating `ReceiptsDrawer.test.tsx:57` and `TradeHistoryTable.test.tsx:97` or tests will fail.
- The CLI flag rename is a breaking operator API change; any existing scripts, CI configs, or user documentation that uses `--gate-epsilon` or `--parent-holdout-score` will break silently (wrong flag name will be ignored or cause an error depending on clap deny_unknown_fields setting).
