# Marketplace Plugin — Design

> **Status:** Draft for user review · 2026-05-09
> **Author:** xianvec hackathon team
> **Companion specs:** [Karpathy Autoresearcher](./2026-05-09-karpathy-autoresearcher-design.md) (the producer of artifacts this plugin consumes) · [Smart Contract Surface](./2026-05-08-smart-contract-surface-design.md) (ERC-8004 registry surface on Mantle)
> **Hackathon deadline:** 2026-06-15 (5 weeks)

---

## 1. Purpose, scope, and persona

The Marketplace Plugin is the **optional Persona B layer** on top of the autoresearcher. It consumes `CycleSeal` artifacts from the autoresearch core and exposes them as ERC-8004 receipts on Mantle. Persona A (the trader/researcher) never installs this plugin and never sees any of its UI. Persona B (the marketplace participant — including hackathon judges) installs it via the `marketplace` cargo feature.

The plugin's job is narrow: **publish what's already provable.** It does not generate new lineage data, does not gate the autoresearch loop, does not modify the core's behavior. It reads sealed artifacts, mints NFTs, posts Merkle roots, and indexes external attestations. Anything else is out of scope.

### 1.1 In scope (v1, by 2026-06-15)

- Cargo feature gate `marketplace` (mirrors `control-vectors` from [ADR 0010](../../decisions/0010-hackathon-pivot-strategy-loom.md))
- Per-lineage ERC-8004 Identity NFT minting (one NFT per *lineage*, not per variant; ~5–10 mints over the hackathon)
- Counterfactual-chain Merkle receipts posted to Reputation Registry (one per anchored lineage; ~5–10 over the hackathon)
- SessionCommitment hash anchored to Reputation Registry at session start (1 tx)
- 1–2 in-house attester agents (each with its own ERC-8004 identity) consuming the local CycleSeal feed and posting ValidationReceipts
- Marketplace dashboard tab (NFT links, attestation viewer, anchor history, operator action panel)
- CLI: `xvn marketplace mint-lineage`, `xvn marketplace anchor`, `xvn marketplace list`, `xvn marketplace attesters status`

### 1.2 Out of scope (v1; deferred to v2)

- Public attestation feed endpoint (open to external participants)
- External attester onboarding flow
- Trust-tier UI (gold/silver/bronze)
- Per-cycle real-time anchoring (v1 anchors at lineage end or on-demand)
- Per-canary on-chain receipts (canary runs locally; not anchored)
- Per-trade validation receipts on closed Orderly trades (already covered by [smart contract surface spec](./2026-05-08-smart-contract-surface-design.md) — orthogonal)
- Marketplace fees, slashing, dispute resolution
- TEE / zkML attestation
- Strategy delegation flow ("one-click delegate"); covered by [strategy-engine 2d dashboard plan](../plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md)

### 1.3 Total chain footprint (v1)

| Source | Count |
|---|---|
| SessionCommitment anchor | 1 |
| Lineage NFT mints | ~5–10 |
| Lineage Merkle receipts | ~5–10 |
| In-house attester ValidationReceipts | ~10–20 (2 attesters × ~5–10 lineages) |
| **Total** | **~20–40 transactions over the hackathon** |

Mantle is cheap; this is a small budget. Pre-fund the operator wallet to 5× estimate before kickoff.

---

## 2. Locked decisions

| # | Decision |
|---|---|
| 1 | **Plugin architecture.** Optional cargo feature `marketplace` in `xianvec-engine`. Core compiles without it. |
| 2 | **One NFT per lineage**, not per variant. Variants are referenced inside the lineage manifest by content hash. |
| 3 | **Lineage-end Merkle anchoring** (or on-demand mid-hackathon) is the default. No per-cycle anchoring in v1. |
| 4 | **In-house attesters seeded for the demo.** xianvec operates 1–2 ERC-8004 attester agents. Public/external participation is v2. |
| 5 | **Subscribes to CycleSeal events** from autoresearch core; never modifies them. Strict one-way data flow. |
| 6 | **Mantle mainnet for the submission**, Sepolia for development. Cutover scripted in Wk 5. |
| 7 | **Operator key separation.** Operator's autoresearch signing key (per autoresearch spec §7) is distinct from the on-chain wallet that holds NFTs. The autoresearch key signs seals; the wallet posts transactions. |

---

## 3. Architecture

### 3.1 Module layout

```
xianvec-engine/
└── src/
    ├── autoresearch/            # core (chain-free) — see autoresearcher spec
    └── marketplace/             # THIS SPEC; only compiled with `--features marketplace`
        ├── mod.rs
        ├── adapter.rs           # AnchorDriver trait; ERC-8004 implementation
        ├── identity.rs          # lineage NFT mint via Identity Registry
        ├── reputation.rs        # Merkle root posting via Reputation Registry
        ├── attesters/           # in-house attester agents
        │   ├── mod.rs
        │   ├── regime_verifier.rs    # checks finding's regime claim against trace
        │   └── diversity_check.rs    # confirms variant adds genuine diversity
        ├── ingest.rs            # subscribes to autoresearch::progress SSE events
        ├── dashboard.rs         # marketplace tab; SSE event handlers for NFT/receipt/attestation
        └── cli.rs               # `xvn marketplace ...` subcommands
```

**Dependency rule:** `marketplace/` may freely import from `autoresearch/`, `eval/`, `strategy/`, `mcp/`. The reverse is forbidden — autoresearch core has no `use crate::marketplace::*` anywhere. CI enforces this with a feature-flag-off build that must succeed.

### 3.2 The AnchorDriver port

Anchoring is abstracted behind a trait so the plugin can be tested without hitting Mantle and so future drivers (e.g., Solana, IPFS-only, signed-but-no-chain) can be slotted in without touching call sites.

```rust
trait AnchorDriver: Send + Sync {
    fn anchor_session_commitment(&self, c: &SessionCommitment) -> Result<TxHash>;
    fn mint_lineage_nft(&self, lineage_id: Ulid, manifest_cid: Cid, parent_lineage_id: Option<Ulid>) -> Result<TokenId>;
    fn post_lineage_merkle(&self, lineage_id: Ulid, merkle_root: B256) -> Result<TxHash>;
    fn post_validation_receipt(&self, attester_id: U256, bundle_hash: ContentHash, verdict: AttestationVerdict, rationale_cid: Cid) -> Result<TxHash>;
}
```

V1 ships one implementation: `Erc8004MantleDriver`. A `MockDriver` exists for tests and for `cargo test --features marketplace` runs.

### 3.3 Subscription to autoresearch core

`ingest.rs` subscribes to `autoresearch::progress` SSE events:

```
cycle_sealed { cycle_id, seal_hash, lineage_edges_added }
   → for each new lineage in lineage_edges_added:
       if first edge in this lineage: identity.mint_lineage_nft(...)
   → schedule attesters to score recent committed bundles
   → no immediate Merkle anchor (lineage Merkle anchors happen on lineage end or on-demand)
```

The subscriber is idempotent. If it crashes mid-cycle and restarts, it scans the `cycle_seals` table and re-derives what should have been minted/posted but wasn't, then catches up.

---

## 4. Lineage NFT minting (Identity Registry)

When a new lineage is born — i.e., the first variant in a lineage commits — the plugin mints one ERC-8004 Identity NFT.

```rust
struct LineageManifest {
    lineage_id: Ulid,
    initial_bundle_hash: ContentHash,
    parent_lineage_id: Option<Ulid>,    // for forks; None for seed lineages
    born_at: DateTime,
    operator_signature: Signature,
    autoresearch_session_id: Ulid,      // links back to the SessionCommitment
}
```

The manifest is uploaded to IPFS (or operator-controlled storage with a content hash); the resulting CID becomes the NFT's `agentURI`. Subsequent variants in the lineage are NOT minted; they are referenced by content hash inside the lineage's append-only mutation log (which itself is anchored later via the Merkle receipt).

This means the on-chain artifact for "lineage" is one NFT + one mutation-log Merkle receipt at the end. Variants are addressable via the manifest but don't each consume a tx.

---

## 5. Counterfactual-chain Merkle receipts (Reputation Registry)

`autoresearch::lineage::compute_merkle_root(lineage_id)` produces:

```
Merkle root over leaves:
  parent_hash → child_hash → days_alive → trades_attributed → realized_pnl_attributed
```

The plugin posts this root to the Reputation Registry along with `lineage_id` and a content hash of the lineage manifest:

```rust
struct LineageReceipt {
    lineage_id: Ulid,
    merkle_root: B256,
    manifest_cid: Cid,
    receipt_kind: ReceiptKind,           // Snapshot | LineageEnd
    posted_at: DateTime,
}

enum ReceiptKind { Snapshot, LineageEnd }
```

V1 supports two anchoring trigger modes:
- **`xvn marketplace anchor <lineage_id>`** — operator-triggered; posts a Snapshot receipt for the current state of the lineage.
- **`xvn marketplace anchor --all-final`** — at hackathon end, posts a LineageEnd receipt for every active lineage.

Anyone reading the on-chain receipt can fetch the manifest from IPFS, fetch the artifact bundle, recompute the Merkle root, and verify. The chain is the timestamp; the artifacts are the proof.

---

## 6. In-house attester agents

Two attester agents demonstrate the open attestation surface for the demo. Each has its own ERC-8004 Identity NFT (minted manually at hackathon kickoff), separate from the operator's main identity.

### 6.1 Regime-verifier agent (`attesters/regime_verifier.rs`)

Reads each new committed Finding. Compares the Finding's claimed `regime_affinity` against the actual regime tags in the variant's trace tape. If the claim matches the trace, posts a `Verdict::Endorse`. If it doesn't, posts a `Verdict::Question` with a one-line rationale (uploaded to IPFS, hash referenced in the receipt).

### 6.2 Diversity-check agent (`attesters/diversity_check.rs`)

Reads each new committed bundle. Computes its embedding distance from existing siblings in the lineage. If the variant adds genuine diversity (above a threshold), posts a `Verdict::Endorse`. If it's a near-clone of an existing sibling, posts `Verdict::Question`.

### 6.3 Attestation receipt schema

```rust
struct AttestationReceipt {
    attester_agent_id: U256,            // attester's ERC-8004 Identity NFT
    bundle_hash: ContentHash,
    verdict: AttestationVerdict,        // Endorse | Question | Reject
    rationale_cid: Cid,                 // pointer to off-chain rationale
    posted_at: DateTime,
    on_chain_tx: TxHash,
}

enum AttestationVerdict { Endorse, Question, Reject }
```

Stored on-chain via the Validation Registry. Indexed off-chain by the marketplace dashboard for fast querying.

### 6.4 Why two in-house attesters

- Demonstrates that **multiple independent signals** can score the same lineage — the marketplace shape is alive.
- Each attester has narrow, well-defined logic, so its verdicts are interpretable and don't read as rubber-stamping.
- Disagreement between attesters is itself rendered (a lineage can be `regime-endorsed` but `diversity-questioned`), proving the system isn't a rubber-stamp ring.
- Cost: ~10–20 tx total over the hackathon. Compute is two LLM calls per committed bundle.

V2 opens this surface to external participants (anyone with an ERC-8004 Identity can post an AttestationReceipt against any `bundle_hash`). V1 fakes it with internal seeding so the surface exists.

---

## 7. Marketplace dashboard tab

A sixth tab in the autoresearch dashboard (only present when the plugin is enabled). Four panels:

1. **Lineage list with NFT links.** One row per lineage. Columns: lineage_id, NFT token_id, parent lineage, birth time, current Sharpe, anchor status (anchored / pending / never).
2. **Per-lineage attestation viewer.** Click a lineage → shows in-house attester verdicts (regime + diversity), with rationale text expandable. Disagreements highlighted.
3. **Anchor history.** Timeline of every Merkle receipt posted, with tx links to Mantle explorer.
4. **Operator action panel.** Buttons: `Mint missing NFTs`, `Anchor lineage <id>`, `Anchor all final`, `Run attesters now`. Each button is gated behind a confirmation modal showing tx cost estimate.

The marketplace tab listens for plugin SSE events: `nft_minted`, `merkle_anchored`, `attestation_posted`, `attester_disagreement`. These are emitted by the plugin's own SSE channel, separate from autoresearch core's channel.

---

## 8. CLI surface

```
xvn marketplace mint-lineage <lineage_id>
    Mints the ERC-8004 Identity NFT for a lineage if not already minted.
    Manifest is uploaded to IPFS and the CID becomes the agentURI.

xvn marketplace anchor <lineage_id> [--snapshot|--final]
    Posts the lineage's counterfactual-chain Merkle root to Reputation Registry.
    --snapshot: receipt_kind = Snapshot (mid-hackathon).
    --final:    receipt_kind = LineageEnd (hackathon end / lineage retirement).

xvn marketplace anchor --all-final
    Convenience: posts LineageEnd receipts for every active lineage. Used at hackathon end.

xvn marketplace list [--include-ghost]
    Lists all lineages with their on-chain status: NFT minted, last anchor, attestation count.

xvn marketplace attesters status
    Shows the two in-house attesters' status: alive/dead, recent verdicts, disagreement count.

xvn marketplace attesters run
    One-shot: forces both attesters to score every recently-committed bundle that hasn't been scored yet.
```

---

## 9. Sequencing (within the autoresearch Wk 5 window)

The marketplace plugin's full work fits in one week (Wk 5: 6/06 → 6/13). Autoresearch core finishes in Wks 1–4 and runs in `--no-default-features` mode through that period. Wk 5 is the integration window.

| Day | Deliverable |
|---|---|
| 6/06 (Mon) | `marketplace` cargo feature scaffolded. `AnchorDriver` trait + `MockDriver` + `Erc8004MantleDriver` skeletons. `xvn marketplace list` works against local store. |
| 6/07 (Tue) | Lineage NFT minting via `Erc8004MantleDriver` on Sepolia. `xvn marketplace mint-lineage` smoke test. |
| 6/08 (Wed) | Counterfactual-chain Merkle receipt posting. `xvn marketplace anchor` smoke test on Sepolia. |
| 6/09 (Thu) | In-house attester agents (regime-verifier + diversity-check). End-to-end on Sepolia. |
| 6/10 (Fri) | Marketplace dashboard tab. Real-time updates from plugin SSE channel. |
| 6/11 (Sat) | **Mainnet cutover.** Switch driver config to Mantle mainnet. Pre-fund operator wallet. Mint attester NFTs. Anchor SessionCommitment. |
| 6/12 (Sun) | Live evening cycle on mainnet. Mint lineage NFTs as lineages emerge. Smoke run for the demo. |
| 6/13 (Mon) | `xvn marketplace anchor --all-final` for submission. Submission packaged. |
| 6/14 (Tue) | Buffer / rehearsal. |
| 6/15 (Wed) | Submit. |

**Wk 5 hard milestones:**
- 6/08: Sepolia integration end-to-end (mint + anchor + attest) demonstrably working.
- 6/11: Mainnet wallet funded, attester NFTs minted, SessionCommitment anchored.
- 6/13: All active lineages have a LineageEnd Merkle receipt on Mantle mainnet.

If the Sepolia integration is not working by 6/09 (Thu), trim scope: drop the in-house attesters (defer to v2), ship lineage NFTs + Merkle receipts only. The demo's marketplace beat becomes "see lineage NFTs and audit the Merkle path"; the attester surface is described but not demonstrated live.

---

## 10. Failure modes and mitigations

Marketplace-specific risks. Loop-side risks live in the [autoresearcher spec §11](./2026-05-09-karpathy-autoresearcher-design.md).

| # | Failure | Mitigation |
|---|---|---|
| 1 | Mainnet cutover breaks on demo day | Sepolia mirror kept fully functional; demo can flip to Sepolia driver via config if mainnet wallet is compromised. Pre-recorded mainnet anchoring screencast as last-resort fallback. |
| 2 | NFT mint flakiness (RPC errors, gas estimation off) | All mints idempotent against local `cycle_seals` table; restart resumes. Conservative gas multiplier (1.5×). Pre-fund 5× estimate. |
| 3 | IPFS pin latency / pin failure | Operator-controlled storage with content hash works as fallback; manifest CID is content-derived so pinning is replaceable. |
| 4 | In-house attesters look like rubber-stamping | Each attester has narrow, mechanical logic (regime claim vs trace; embedding distance threshold). Disagreements are rendered, not hidden. Public rationale text on IPFS. |
| 5 | Judges think 1–2 attesters is too few to be a "marketplace" | Submission write-up explicitly frames v1 as "in-house seeded for demonstration; v2 opens the surface to external participants." Honesty over puffery. |
| 6 | Merkle root posted, but verification fails (manifest missing, wrong hash) | Anchor is idempotent and the manifest is content-addressed; if verification fails, post the corrected manifest CID and re-anchor. Old anchor is left as historical (signed bytes don't lie). |
| 7 | Per-tx cost on Mantle mainnet higher than estimated | Conservative pre-fund (5×). Mantle is L2; even worst-case is small. Budget alarm in the dashboard if wallet balance falls below threshold. |
| 8 | "It's not really decentralized — xianvec runs the attesters" | True for v1; the plugin architecture explicitly enables external participation in v2. The submission write-up calls this out as a design feature, not a hack. |
| 9 | A lineage's Merkle root differs between local computation and on-chain receipt | Single source of truth: `autoresearch::lineage::compute_merkle_root`. Plugin imports it, never re-implements. Test asserts byte-identical roots between core and plugin. |
| 10 | Wallet key compromised during hackathon | Wallet is operator-controlled hot wallet with limited balance (just enough for hackathon ops). Cold storage of upgrade keys; v2 work after submission. |

---

## 11. Open questions (to resolve in the implementation plan)

1. **IPFS pinning provider** — Pinata vs Web3.Storage vs Filebase vs operator-self-hosted. Resolve in Wk 5 day 1.
2. **Attester wallet key custody** — separate wallets per attester or shared? Single wallet is simpler; separate is cleaner. Default to separate.
3. **Mantle gas oracle** — use Mantle's gas-price oracle or a fixed multiplier? Test both during Sepolia week.
4. **Anchor receipt format** — flat (just root) or include manifest metadata (lineage_id, kind, timestamp) on-chain? Trade-off: gas cost vs on-chain queryability. Default to including metadata; revisit if gas is unexpectedly high.
5. **Attestation throttling** — should attesters score every committed bundle or sample? Default to every bundle in v1 (volume is low); revisit if cycle budgets grow.

---

## 12. References

- [Karpathy Autoresearcher spec](./2026-05-09-karpathy-autoresearcher-design.md) (the producer of CycleSeal artifacts this plugin consumes; defines persona split)
- [Smart Contract Surface spec](./2026-05-08-smart-contract-surface-design.md) (ERC-8004 registry deployment, contract interfaces, ABI)
- [ADR 0010 — Hackathon Pivot](../../decisions/0010-hackathon-pivot-strategy-loom.md) (cargo-feature-gate idiom; marketplace-as-surface framing)
- ERC-8004 EIP — eips.ethereum.org/EIPS/eip-8004 (mainnet live 2026-01-29)
- Mantle Turing Test Hackathon 2026 (Phase 2, "AI Awakening")
