# Marketplace Plugin — Design

> **Status:** Accepted plugin surface, pending implementation in the marketplace
> program. Scheduling is superseded by
> [`2026-05-26-marketplace-program-strategy.md`](../plans/2026-05-26-marketplace-program-strategy.md):
> V2 is Mantle Sepolia testnet only; mainnet is V4. · originally drafted
> 2026-05-09
> **Author:** xvision team
> **Companion specs:** [Karpathy Autoresearcher](./2026-05-09-karpathy-autoresearcher-design.md) (the producer of artifacts this plugin consumes) · [Smart Contract Surface](./2026-05-08-smart-contract-surface-design.md) (ERC-8004 + marketplace contract surface on Mantle Sepolia for V2)
> **Hackathon deadline:** superseded; historical schedule retained only as old context where explicitly marked.
>
> **Amended 2026-05-26** by [`docs/superpowers/plans/2026-05-26-marketplace-design-direction.md`](../plans/2026-05-26-marketplace-design-direction.md). Three points:
> 1. **Persona A vs Persona B surface split.** The "Marketplace dashboard tab" described in §7 is the **Persona A operator surface** — it lives inside the self-hosted XVN dashboard for the operator who minted the lineages and runs the chain ops. The **Persona B public marketplace** (browse, identity pages, leaderboards, creator profiles, buy/clone-to-edit) is a separate surface and is owned by the direction doc, not this spec. Don't mix them.
> 2. **Decision #2 (one NFT per lineage) is now canonical** across this spec and the [smart contract surface](./2026-05-08-smart-contract-surface-design.md). The surface spec has been amended (§3.1.1) to adopt this position, resolving the A4 conflict that the blockchain nav doc flagged.
> 3. **Operator action panel in §7** (Mint missing NFTs / Anchor lineage / Anchor all final / Run attesters now) belongs in Settings → Chain ops, NOT on the public marketplace. The public marketplace surface stays buyer/seller-focused per the direction doc. The PDF design `XVN · Blockchain surfaces.pdf` (referenced from the direction doc context) is essentially this Persona A surface; the direction doc replaces it for Persona B.
> 4. **Testnet-only through V2.** Mentions of Mantle mainnet submission,
>    6/11 cutover, and hackathon milestones are stale.
> 5. **IPFS storage is no longer an open provider pick.** V2 ships Pinata-only
>    behind an `IpfsStore` trait; `iroh` install-mesh is V3.
> 6. **No popups.** Operator confirmations use an inline/dock/route surface, not
>    a modal.

---

## 1. Purpose, scope, and persona

The Marketplace Plugin is the **Persona A operator / chain-ops layer** on top of
the autoresearcher. It consumes `CycleSeal` artifacts from the autoresearch core
and exposes them as ERC-8004 receipts on Mantle Sepolia for V2. Marketplace is
part of the default xvn build; Persona A always sees chain ops in Settings but
never has to publish. Persona B buyer/seller marketplace UX is owned by the
direction doc and Phase-F frontend spec, not this plugin spec.

The cargo feature gate `marketplace` exists for build-flexibility (size-conscious / audit / minimal builds) but is enabled by default. The user-facing opt-in is the wallet-connect step in Settings, not a recompile. Mirroring the framing from [ADR 0010](../../decisions/0010-hackathon-pivot-strategy-loom.md): the feature gate handles build choice, the wallet-connect handles user choice.

The plugin's job is narrow: **publish what's already provable.** It does not generate new lineage data, does not gate the autoresearch loop, does not modify the core's behavior. It reads sealed artifacts, mints NFTs, posts Merkle roots, and indexes external attestations. Anything else is out of scope.

### 1.1 In scope (first plugin slice, V2 testnet)

- Cargo feature gate `marketplace` for build-flexibility (default builds include it; minimal/audit builds can opt out via `--no-default-features`)
- Per-lineage ERC-8004 Identity NFT minting (one NFT per *lineage*, not per variant)
- Counterfactual-chain Merkle receipts posted to Reputation Registry (one per anchored lineage)
- SessionCommitment hash anchored to Reputation Registry at session start (1 tx)
- 1–2 in-house attester agents (each with its own ERC-8004 identity) consuming the local CycleSeal feed and posting ValidationReceipts
- Marketplace dashboard tab (NFT links, attestation viewer, anchor history, operator action panel)
- CLI: `xvn marketplace mint-lineage`, `xvn marketplace anchor`, `xvn marketplace list`, `xvn marketplace attesters status`

### 1.2 Out of scope (first plugin slice; later V2/V3/V4 work)

- Public attestation feed endpoint (open to external participants)
- External attester onboarding flow
- Trust-tier UI (gold/silver/bronze)
- Per-cycle real-time anchoring (first slice anchors at lineage end or on-demand)
- Per-canary on-chain receipts (canary runs locally; not anchored)
- Per-trade validation receipts on closed Orderly trades (already covered by [smart contract surface spec](./2026-05-08-smart-contract-surface-design.md) — orthogonal)
- Marketplace fees, slashing, dispute resolution
- TEE / zkML attestation
- Strategy delegation flow ("one-click delegate"); covered by [strategy-engine 2d dashboard plan](../plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md)

### 1.3 Total chain footprint (historical estimate; rescope during Phase 5)

| Source | Count |
|---|---|
| SessionCommitment anchor | 1 |
| Lineage NFT mints | depends on published fixture/demo set |
| Lineage Merkle receipts | depends on published fixture/demo set |
| In-house attester ValidationReceipts | depends on attester scope |
| **Total** | rescope during Phase 5 planning |

Mantle Sepolia is cheap; pre-fund the operator wallet to 5× the Phase-5
estimate before testnet runs.

---

## 2. Locked decisions

| # | Decision |
|---|---|
| 1 | **Marketplace is part of default xvn build, opt-in at wallet-connect.** Cargo feature `marketplace` in `xvision-engine` is on by default; available to opt out for minimal builds (`--no-default-features`). User-facing opt-in is the Settings → Marketplace wallet-connect step. |
| 2 | **One NFT per lineage**, not per variant. Variants are referenced inside the lineage manifest by content hash. |
| 3 | **Lineage-end Merkle anchoring** (or on-demand snapshots) is the default. No per-cycle anchoring in the first slice. |
| 4 | **In-house attesters seeded for V2.** xvision operates 1–2 ERC-8004 attester agents. Public/external participation is later work. |
| 5 | **Subscribes to CycleSeal events** from autoresearch core; never modifies them. Strict one-way data flow. |
| 6 | **Mantle Sepolia only through V2.** Mainnet is V4 after the V2 exit gate, audit, and governance prep. |
| 7 | **Operator key separation.** Operator's autoresearch signing key (per autoresearch spec §7) is distinct from the on-chain wallet that holds NFTs. The autoresearch key signs seals; the wallet posts transactions. |

---

## 3. Architecture

### 3.1 Module layout

```
xvision-engine/
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
        ├── dashboard.rs         # Settings → Chain ops handlers for NFT/receipt/attestation
        └── cli.rs               # `xvn marketplace ...` subcommands
```

**Dependency rule:** `marketplace/` may freely import from `autoresearch/`, `eval/`, `strategy/`, `mcp/`. The reverse is forbidden — autoresearch core has no `use crate::marketplace::*` anywhere. CI enforces this with a feature-flag-off build that must succeed.

### 3.2 The AnchorDriver port

Anchoring is abstracted behind a trait so the plugin can be tested without
hitting Mantle and so future drivers (e.g., Solana, IPFS-only,
signed-but-no-chain) can be slotted in without touching call sites. Storage is
separate: manifests/rationales are written through an `IpfsStore` abstraction.
V2 ships a `PinataDriver`; `iroh` install-mesh is V3.

```rust
trait IpfsStore: Send + Sync {
    fn put_json<T: Serialize>(&self, value: &T) -> Result<Cid>;
    fn put_bytes(&self, bytes: &[u8], content_type: &str) -> Result<Cid>;
}

trait AnchorDriver: Send + Sync {
    fn anchor_session_commitment(&self, c: &SessionCommitment) -> Result<TxHash>;
    fn mint_lineage_nft(&self, lineage_id: Ulid, manifest_cid: Cid, parent_lineage_id: Option<Ulid>) -> Result<TokenId>;
    fn post_lineage_merkle(&self, lineage_id: Ulid, merkle_root: B256) -> Result<TxHash>;
    fn post_validation_receipt(&self, attester_id: U256, bundle_hash: ContentHash, verdict: AttestationVerdict, rationale_cid: Cid) -> Result<TxHash>;
}
```

V2 ships one anchor implementation: `Erc8004MantleDriver`, plus a `MockDriver`
for tests and `cargo test --features marketplace` runs.

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

The manifest is uploaded through `IpfsStore`; the resulting CID becomes the
NFT's `agentURI`. Subsequent variants in the lineage are NOT minted; they are
referenced by content hash inside the lineage's append-only mutation log (which
itself is anchored later via the Merkle receipt).

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
- **`xvn marketplace anchor --all-final`** — posts a LineageEnd receipt for every selected/final lineage.

Anyone reading the on-chain receipt can fetch the manifest from IPFS, fetch the artifact bundle, recompute the Merkle root, and verify. The chain is the timestamp; the artifacts are the proof.

---

## 6. In-house attester agents

Two attester agents demonstrate the open attestation surface for V2. Each has
its own ERC-8004 Identity NFT, separate from the operator's main identity.

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
- Cost depends on the selected V2 demo set. Compute is two LLM calls per committed bundle.

V2 opens this surface to external participants (anyone with an ERC-8004 Identity can post an AttestationReceipt against any `bundle_hash`). V1 fakes it with internal seeding so the surface exists.

---

## 7. Marketplace dashboard tab

> **Scope clarification (amended 2026-05-26).** This section describes the **Persona A operator surface** — the dashboard tab inside the self-hosted XVN install for the operator who minted the lineages. It is *not* the public buyer/seller marketplace, which is owned by [`2026-05-26-marketplace-design-direction.md`](../plans/2026-05-26-marketplace-design-direction.md) and lives in a separate thin read-only public viewer. The four panels below are operator-facing: anchor status, attester verdicts, anchor history, operator actions (mint / anchor / run attesters). The public Persona B surface has different columns, different defaults, and no operator-action panel — see direction doc §4.

A sixth tab in the autoresearch dashboard (only present when the plugin is enabled). Four panels:

1. **Lineage list with NFT links.** One row per lineage. Columns: lineage_id, NFT token_id, parent lineage, birth time, current Sharpe, anchor status (anchored / pending / never).
2. **Per-lineage attestation viewer.** Click a lineage → shows in-house attester verdicts (regime + diversity), with rationale text expandable. Disagreements highlighted.
3. **Anchor history.** Timeline of every Merkle receipt posted, with tx links to Mantle explorer.
4. **Operator action panel.** Buttons: `Mint missing NFTs`, `Anchor lineage <id>`, `Anchor all final`, `Run attesters now`. Each action expands an inline confirmation row / dock showing tx cost estimate; no modal or popup.

The Settings → Chain ops surface listens for plugin SSE events: `nft_minted`,
`merkle_anchored`, `attestation_posted`, `attester_disagreement`. These are
emitted by the plugin's own SSE channel, separate from autoresearch core's
channel.

---

## 8. CLI surface

```
xvn marketplace mint-lineage <lineage_id>
    Mints the ERC-8004 Identity NFT for a lineage if not already minted.
    Manifest is uploaded to IPFS and the CID becomes the agentURI.

xvn marketplace anchor <lineage_id> [--snapshot|--final]
    Posts the lineage's counterfactual-chain Merkle root to Reputation Registry.
    --snapshot: receipt_kind = Snapshot (mid-run / operator checkpoint).
    --final:    receipt_kind = LineageEnd (lineage retirement / final checkpoint).

xvn marketplace anchor --all-final
    Convenience: posts LineageEnd receipts for every selected/final lineage.

xvn marketplace list [--include-ghost]
    Lists all lineages with their on-chain status: NFT minted, last anchor, attestation count.

xvn marketplace attesters status
    Shows the two in-house attesters' status: alive/dead, recent verdicts, disagreement count.

xvn marketplace attesters run
    One-shot: forces both attesters to score every recently-committed bundle that hasn't been scored yet.
```

---

## 9. Sequencing (superseded)

The original week-by-week hackathon schedule is obsolete. Current sequencing is
owned by
[`2026-05-26-marketplace-program-strategy.md`](../plans/2026-05-26-marketplace-program-strategy.md):

1. Phase F builds the fixture-backed marketplace UI and data seam first.
2. Phase 1 closes the metadata/data-contract spec from that seam.
3. Phase 3 deploys ERC-8004 testnet stubs and the deterministic deployer.
4. Phase 5 implements this plugin together with marketplace contracts, the
   subgraph, `IpfsStore`, CLI verbs, and the Settings → Chain ops API surface.
5. Phase 6 wires the frontend to real backends and wallet flows.

There is **no Mantle mainnet cutover in V2**. Mainnet is V4 after the V2 exit
gate, audit, and governance prep.

---

## 10. Failure modes and mitigations

Marketplace-specific risks. Loop-side risks live in the [autoresearcher spec §11](./2026-05-09-karpathy-autoresearcher-design.md).

| # | Failure | Mitigation |
|---|---|---|
| 1 | Sepolia deployment or RPC breaks during V2 testnet work | Keep `MockDriver` and local anvil paths working; retry via alternate Mantle Sepolia RPC; no mainnet fallback in V2. |
| 2 | NFT mint flakiness (RPC errors, gas estimation off) | All mints idempotent against local `cycle_seals` table; restart resumes. Conservative gas multiplier (1.5×). Pre-fund 5× estimate. |
| 3 | IPFS pin latency / pin failure | Operator-controlled storage with content hash works as fallback; manifest CID is content-derived so pinning is replaceable. |
| 4 | In-house attesters look like rubber-stamping | Each attester has narrow, mechanical logic (regime claim vs trace; embedding distance threshold). Disagreements are rendered, not hidden. Public rationale text on IPFS. |
| 5 | 1–2 attesters look too narrow | Product copy frames this as in-house seeded validation for V2; external attesters are later work. Honesty over puffery. |
| 6 | Merkle root posted, but verification fails (manifest missing, wrong hash) | Anchor is idempotent and the manifest is content-addressed; if verification fails, post the corrected manifest CID and re-anchor. Old anchor is left as historical (signed bytes don't lie). |
| 7 | Per-tx cost higher than estimated | Conservative Sepolia pre-fund (5× estimate) and budget alarm in the dashboard if wallet balance falls below threshold. |
| 8 | "It's not really decentralized — xvision runs the attesters" | True for the first V2 slice; the plugin architecture explicitly enables later external participation. Product copy calls this out as a design feature, not a hack. |
| 9 | A lineage's Merkle root differs between local computation and on-chain receipt | Single source of truth: `autoresearch::lineage::compute_merkle_root`. Plugin imports it, never re-implements. Test asserts byte-identical roots between core and plugin. |
| 10 | Operator wallet key compromised during V2 testnet | Wallet is operator-controlled EOA with limited Sepolia funds. V4 moves upgrade authority to timelock + multisig before mainnet. |

---

## 11. Open questions (to resolve in the implementation plan)

1. **IPFS pinning architecture** — resolved by the marketplace strategy: V2
   ships Pinata-only behind `IpfsStore`; `iroh` install-mesh is V3.
2. **Attester wallet key custody** — separate wallets per attester or shared? Single wallet is simpler; separate is cleaner. Default to separate.
3. **Mantle gas oracle** — use Mantle's gas-price oracle or a fixed multiplier? Test both during Sepolia week.
4. **Anchor receipt format** — flat (just root) or include manifest metadata (lineage_id, kind, timestamp) on-chain? Trade-off: gas cost vs on-chain queryability. Default to including metadata; revisit if gas is unexpectedly high.
5. **Attestation throttling** — should attesters score every committed bundle or sample? Default to every bundle in the first slice (volume is low); revisit if cycle budgets grow.

---

## 12. References

- [Karpathy Autoresearcher spec](./2026-05-09-karpathy-autoresearcher-design.md) (the producer of CycleSeal artifacts this plugin consumes; defines persona split)
- [Smart Contract Surface spec](./2026-05-08-smart-contract-surface-design.md) (ERC-8004 registry deployment, contract interfaces, ABI)
- [ADR 0010 — Hackathon Pivot](../../decisions/0010-hackathon-pivot-strategy-loom.md) (cargo-feature-gate idiom; marketplace-as-surface framing)
- ERC-8004 EIP — eips.ethereum.org/EIPS/eip-8004 (mainnet live 2026-01-29)
- Mantle Turing Test Hackathon 2026 (Phase 2, "AI Awakening")
