# Auto-improving memory for xvision — survey & recommendation

Status: research brief
Date: 2026-05-11
Author: research session w/ Edward

## TL;DR

Build xvision's memory layer as a **Rust-native, in-process system** on
top of **LanceDB + fastembed-rs**, with custom Rust modules for the
auto-improvement loop (outcome learning, consolidation, contradiction
resolution) calling a local LLM (Ollama) for the reflective passes.

Two existing projects deserve a hard look before committing to the
DIY foundation:

1. **gambletan/cortex** — pure Rust (3.8 MB binary), 4-tier memory,
   HNSW, AES-256-GCM sync, **73.7 % LoCoMo (beats mem0 at 66.9 %)**.
   Closest thing to a drop-in Rust answer.
2. **cognee-RS** — official Rust SDK from cognee for edge/on-device
   memory, targets sub-100 ms local recall with Phi-4-class models.
   Experimental but from a funded project with serious roadmap.

Both rule out mem0 / Letta / Hermes / Graphiti as primary memory
infrastructure for xvision, because each of those is Python-only
sidecar service and adds latency + ops surface that conflicts with
"xvn ships as a single binary." Mem0 / Graphiti can still play a
**secondary, post-cycle role** for batch consolidation of EOD reports
and autoresearcher output if needed — but that's a Phase 2 question.

The user's two named candidates fared as follows:
- **Hermes Agent (NousResearch)**: real and impressive, but it is an
  *autonomous agent that has memory*, not memory infrastructure for
  other agents. Wrong abstraction for xvision.
- **mem0**: real "self-improving" mechanism (LLM-judged
  ADD/UPDATE/DELETE/NOOP, contradictions trigger DELETE). Best-in-class
  scoping primitives (`user_id`/`agent_id`/`run_id` map almost 1:1 to
  our model). But Python-only sidecar, adds 1–2 LLM calls per write,
  no native Rust client.

## Context

xvision is a Rust workspace running a multi-agent trading pipeline
(intern → trader → risk → executor) plus periodic activities
(autoresearcher, EOD reports, scheduled tasks). The marketplace
roadmap puts a `StrategyBundle` behind each `agent_id` (ULID, later
NFT token ID); each cycle (briefing → decision → outcome) is keyed by
`cycle_id`.

Requirements for memory:

| Axis | Requirement |
|---|---|
| Scope | per-`cycle_id`, per-`agent_id`, and global (user/config/EOD/tasks/autoresearcher) |
| Self-host | Yes — xvn users run it themselves, no 3rd-party paid services |
| Stack | Rust-native preferred; sidecar acceptable if compelling |
| Learns from outcomes | Yes — absorb briefing → decision → outcome tuples after each cycle |
| Consolidation | Yes — periodic dedupe / pattern extraction / pruning |
| Belief updates | Yes — detect contradictions, supersede stale facts (not append-only) |
| Latency | Hot path (intern → trader → risk → executor) must stay fast; reflective passes can be async |

## Candidates evaluated

### Self-hostable memory frameworks (sidecar)

**mem0** (Apache-2.0, Python, github.com/mem0ai/mem0)
Strong scoping (`user_id`/`agent_id`/`run_id` maps to xvision exactly).
Real LLM-judged contradiction resolution in code. Backends: Qdrant,
Chroma, Milvus, pgvector, Weaviate, Redis, etc. Local stack with
Qdrant + Ollama works but is brittle (see issue #2030). REST sidecar;
no Rust client. ~150 ms search p50; writes cost 1–2 LLM calls.

**Hermes Agent** (MIT, Python+TS, github.com/NousResearch/hermes-agent)
Built-in MEMORY.md / USER.md + SQLite/FTS5; eight pluggable memory
provider plugins (Honcho, OpenViking, Mem0, Hindsight, Holographic,
RetainDB, ByteRover, Supermemory). The Hindsight provider has
explicit contradiction handling via a `fact_store` `contradict`
action. But Hermes is a full agent runtime — namespacing is per
*profile*, not per programmatically-spawned `agent_id`. Wrong
abstraction; the *components* (Hindsight, FTS5+SQLite pattern) are
more interesting than the framework itself.

**Letta** (Apache-2.0, Python on Postgres+pgvector)
Memory-OS architecture with core/archival/conversational tiers,
`learning-sdk` for outcome learning, agent can rewrite core memory
blocks. Strong but full-stack agent runtime — would fight xvision's
Rust pipeline.

**Graphiti** (Apache-2.0, Python on Neo4j/FalkorDB/Kuzu)
The graph-memory engine extracted from Zep (Zep Community Edition is
now deprecated). Best-in-class **temporal "valid-from /
invalidated-at" edges** — cleanest belief-update model in the
landscape. Compelling as a secondary store for the marketplace
identity layer where strategy beliefs supersede each other over time.

**MIRIX** (Apache-2.0, Python, Mirix-AI/MIRIX)
Six-component schema (Core / Episodic / Semantic / Procedural /
Resource / Knowledge Vault) managed by dedicated sub-agents. SOTA on
LoCoMo (85.4 %). The taxonomy maps cleanly onto xvision concepts
(Procedural = `StrategyBundle` behavior; Episodic = cycle traces;
Semantic = autoresearcher findings; Resource/Vault = EOD reports &
config). Heavy sidecar.

**cognee** (Apache-2.0, Python with Rust SDK in progress)
Hybrid embeddings+graph "memory control plane." Multi-tenant.
Crucial: **`cognee-RS` is an experimental Rust SDK** targeting
on-device memory with Phi-4-class local models and sub-100 ms recall.
Watch this closely; could become the strongest fit if the Rust port
matures.

Skipped: A-MEM (research code), txtai (retrieval lib, no memory
semantics), Memary (stalled), LangMem (LangGraph lock-in, ~60 s p95),
LlamaIndex memory blocks (locked into LlamaIndex), Memoripy
(single-process, no scoping), MemOS (heaviest stack, overkill), memU
(young, small community).

### Rust-native foundations

**gambletan/cortex** (Pure Rust, ~3.8 MB binary)
4-tier memory (working / episodic / semantic / procedural) with
auto-consolidation, decay, multi-hop retrieval, negation detection,
HNSW vector index, AES-256-GCM encrypted sync to user's own
iCloud/GDrive/Dropbox. **73.7 % on LoCoMo, beats mem0 (66.9 %).**
62 µs ingest, 91 µs search at 50 k. Single maintainer is the main
risk; no track record in financial systems; license & API stability
need a closer read before commit.

**rig-rs** (MIT, github.com/0xPlaygrounds/rig)
Rust LLM/agent framework with trait-based `VectorStore` /
`EmbeddingModel` abstractions across ~17 backends. Not a memory
system — but useful as the abstraction layer xvision agents already
talk to LLMs through, and the natural place to plug in whatever
memory backend wins.

**swiftide** (MIT, github.com/bosun-ai/swiftide)
Streaming RAG pipelines for Rust. Good "ingest + retrieve" plumbing;
no memory semantics. Probably duplicative if we have rig + LanceDB.

**LanceDB Rust client** (Apache-2.0, github.com/lancedb/lancedb)
Embedded columnar vector store. Vector ANN + BM25 FTS + hybrid /
RRF / ColBERT reranking + expression filters + **automatic versioning
with zero-copy time travel**. Pure file-based; drops into the same
data dir as xvision's SQLite tables. No graph, no agent semantics —
but that's a feature here, those parts belong in xvision-owned code.

**Qdrant Rust client** (Apache-2.0)
Best-in-class vector + hybrid + payload filtering, but talks gRPC to
a server — violates "no extra service." `EdgeShard` for embedded
exists but lags the main product. Skip for now.

**SurrealDB 3.0** (BSL → Apache-2.0 after 4 years)
The only candidate with KV + document + graph (`RELATE`) + vector
(HNSW) + BM25 FTS + temporal in one queryable surface. Embeddable
Rust binary. **Concern:** open issue #6949 — Rust SDK vector queries
return unfiltered table results in v3.0. Verify before adopting.
Revisit in 6 months — if the graph layer helps model
outcome → belief → contradiction chains, migration cost behind a rig
trait is low.

**fastembed-rs** (Apache-2.0, Anush008/fastembed-rs)
ONNX-backed local embeddings + rerankers (nomic-embed-text-v2-MoE, etc.).
Default choice for embeddings — pairs with any store above.

## Comparison matrix

| Project | Lang | Self-host | Scope model | Learns-from-outcome | Consolidation | Belief-update | Embedded? | xvision verdict |
|---|---|---|---|---|---|---|---|---|
| mem0 | Python | Yes (REST) | user/agent/run_id | Yes (LLM extract) | Partial | Yes (LLM judge) | No | Secondary (post-cycle batch) |
| Hermes Agent | Python+TS | Yes (CLI) | Per-profile | Skill creation | Yes (provider) | Yes (Hindsight) | No | Skip (wrong abstraction) |
| Letta | Python | Yes (PG) | Strong agent IDs | Yes (learning-sdk) | Yes (archival) | Partial | No | Skip (runtime fight) |
| Graphiti | Python | Yes (graph DB) | group_id/user_id | Indirect | Entity res | **Best (temporal edges)** | No | Watch (marketplace ID layer) |
| MIRIX | Python | Yes | **6 typed stores** | Yes | Yes | Yes | No | Watch (schema inspiration) |
| cognee + cognee-RS | Py + **Rust** | Yes | Tenant/dataset | Yes | Yes | Yes | **Rust SDK in flight** | Watch closely |
| **gambletan/cortex** | **Rust** | Yes | 4-tier | Yes | Yes (auto) | Implicit (decay+negation) | **Yes (3.8 MB)** | **Strong contender** |
| rig-rs | Rust | Yes | Short-term only | DIY | DIY | DIY | Yes | **Abstraction layer** |
| LanceDB | Rust | Yes | DIY tables | DIY | DIY | DIY (versioning) | **Yes** | **Storage foundation** |
| SurrealDB | Rust | Yes | DIY | DIY | DIY | DIY | Yes | Revisit in 6 mo |

## Recommended architecture for xvision

A **two-tier memory system**, both running in-process inside `xvn`:

### Tier 1 — Hot path (sync, low latency, in-process)

Storage: **LanceDB** (embedded, file-based, alongside SQLite cycles
tables). Tables:

- `memory_cycle` — per-`cycle_id` working memory; ephemeral.
- `memory_agent` — per-`agent_id` long-term memory for each
  `StrategyBundle`; survives across cycles.
- `memory_global` — user config, EOD reports, scheduled-task
  outputs, autoresearcher findings; queryable by tag.

Embeddings: **fastembed-rs** with `nomic-embed-text` (no Python).

Retrieval: hybrid (BM25 + dense + RRF rerank, all native in LanceDB
Rust client).

Trait surface: define memory traits inside `xvision-memory` crate;
implement against LanceDB. Use **rig-rs** abstractions where they fit
so we can swap storage later.

### Tier 2 — Reflective passes (async, scheduled)

Three jobs that run as scheduled tasks (existing 2c scheduler):

1. **Outcome ingestion**: when a cycle closes (executor reports
   `RealizedReturn`), extract structured `(briefing, decision,
   outcome)` tuples via a local LLM (Ollama, `llama3.x-instruct`)
   and write them to `memory_agent` tagged with realized return,
   risk verdict, and time horizon.

2. **Consolidation**: nightly job pulls last-N rows from
   `memory_agent`, clusters semantically, asks the local LLM to
   merge/summarize into higher-level beliefs, writes a new LanceDB
   version, marks originals as superseded.

3. **Contradiction resolver**: on every write to `memory_agent` or
   `memory_global`, top-k similar prior memories; if cosine ≥ τ AND
   semantic content disagrees (LLM judge), choose between:
   `keep_both_with_disputes_edge`, `update_confidence`, or
   `supersede`. LanceDB versioning gives us free time-travel over
   the table, which is the cleanest mechanism for "valid-from /
   invalidated-at" semantics without bolting on a graph DB.

### What to read from existing OSS before building Tier 2

- **gambletan/cortex** source — for the 4-tier decay / consolidation
  algorithms and negation-detection. Likely the cleanest Rust
  reference in the field; may serve as a dependency if the API and
  license check out, or as a design source if not.
- **mem0** `update_memory` prompt + ADD/UPDATE/DELETE/NOOP operator
  — the contradiction-resolution prompt template is reusable.
- **Graphiti** `valid-from/invalidated-at` edge model — cleanest
  belief-update semantics in the landscape; map onto LanceDB row
  versions rather than introducing a graph DB.
- **MIRIX** 6-store taxonomy — informs how we split
  `memory_agent` rows (Procedural vs Episodic vs Semantic vs
  Resource).

### Why not just adopt gambletan/cortex wholesale?

It's the strongest single Rust answer, but three concerns argue for
treating it as a *reference + possible dependency* rather than the
foundation:

1. **Single maintainer.** xvision is a financial system; we need
   long-term stability of the memory store. LanceDB has institutional
   backing; cortex does not (yet).
2. **API stability unknown.** Cortex is at v1.x with recent rapid
   feature additions (negation detection, multi-hop, query expansion
   shipped within the last few weeks). Pinning to it before APIs
   settle is risky.
3. **Trading-specific scoping.** Cortex's 4 tiers don't natively
   carry `cycle_id` / `agent_id` semantics; we'd have to add them
   anyway. The amount of code we'd write to fit cortex equals or
   exceeds the LanceDB-direct path.

If, after a closer source read and a 2-day spike, cortex's
consolidation + decay code is good enough to depend on directly, we
can swap it in behind the `xvision-memory` trait without changing
callers. **That's the right experiment to run first.**

## Open questions / next steps

1. **Cortex spike** (1–2 days): read source for license details,
   API stability signals, dependency surface. If clean, prototype
   `xvision-memory` trait backed by cortex; if not, proceed with
   LanceDB-direct.
2. **cognee-RS check**: pin the current state of the cognee-RS
   Rust SDK — if it's already shipping basic recall, evaluate it as
   a third option.
3. **Schema design**: how exactly do `cycle_id` / `agent_id` rows
   relate to user-global rows when an autoresearcher finding is
   relevant to a future trade? Plausible answer: cross-table
   metadata pointers + `tag` columns.
4. **Local LLM choice**: which model drives extraction /
   consolidation / contradiction judging? Likely a small
   instruction-tuned Llama or Mistral served by Ollama. Needs a
   benchmark on how well it judges contradictions on real cycle
   data.
5. **Mem0 as secondary?** If trading-cycle memory works well
   in-process, is there still a role for mem0 as a Phase-2 sidecar
   for cross-strategy or marketplace-level memory? Defer — answer
   after Tier 1 is working.

## Sources

- mem0: https://github.com/mem0ai/mem0, paper https://arxiv.org/html/2504.19413v1
- Hermes Agent: https://github.com/NousResearch/hermes-agent, docs https://hermes-agent.nousresearch.com/docs/
- Letta: https://github.com/letta-ai/letta
- Graphiti: https://github.com/getzep/graphiti
- MIRIX: https://github.com/Mirix-AI/MIRIX
- cognee + cognee-RS: https://github.com/topoteretes/cognee, blog https://www.cognee.ai/blog/cognee-news/cognee-rust-sdk-for-edge
- gambletan/cortex: https://github.com/gambletan/cortex, HN https://news.ycombinator.com/item?id=47501353
- rig-rs: https://github.com/0xPlaygrounds/rig
- swiftide: https://github.com/bosun-ai/swiftide
- LanceDB: https://github.com/lancedb/lancedb
- SurrealDB: https://github.com/surrealdb/surrealdb (issue #6949 https://github.com/surrealdb/surrealdb/issues/6949)
- fastembed-rs: https://github.com/Anush008/fastembed-rs
- LoCoMo benchmark: https://snap-research.github.io/locomo/
