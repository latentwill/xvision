# Follow-ups — operational queue

Tactical work deferred during Phase 4–8 implementation. Not strategic
re-examinations (those live in `decisions/strategy-choices.md`); these are
scheduled tasks with a clear trigger or phase that should pick them up.

Format: title → trigger → scope → blocking?

## Track classification (post-2026-05-05 hackathon pivot — see ADR 0010)

After ADR 0010 (Strategy Loom + ERC-8004 marketplace pivot), this queue runs
on three tracks. Existing F-numbers are preserved as historical anchors. New
hackathon work uses the **SLF** series.

| Track | Items | Lives on |
|---|---|---|
| **SLF — Strategy Loom** | new SLF1–16 (below); supersedes F4, F14, F15, F17, F23 | `hackathon/turing` until Jun 15, then merged to `main` |
| **CVF — Control Vector** | F1, F2, F3 (landed), F9, F10, F11, F12, F13, F16, F26, F27, F28, F29, F30, F31, F32 | `main`, behind `--features control-vectors` after SLF12 lands |
| **Shared** | F5, F6, F7, F8, F18, F19, F20, F21 (landed partial), F22, F24, F25 | `main` |

Quick navigation: [SLF queue](#strategy-loom-queue-slf) ·
[CVF queue](#control-vector-queue-cvf) · [Shared queue](#shared-queue)

---

## Strategy Loom queue (SLF)

The hackathon sprint queue. Branch: `hackathon/turing`. Submission deadline:
**2026-06-15**. See ADR 0010 + `LatentWill/Xianvec/pivot1-strategyloom.md`.

### SLF1. Cut `hackathon/turing` branch + initial scaffolding

- **Trigger:** ADR 0010 ratified (done, 2026-05-05).
- **Scope:** branch off `main`. Commit ADR 0010 + `pivot1-strategyloom.md` (done) + this FOLLOWUPS restructure (done). Smoke `cargo build --workspace` on the new branch to confirm parity with `main`.
- **Blocking:** YES for everything else on the SLF track.

### SLF2. Execute ADR 0008 ops runbook on Mantle Sepolia

- **Trigger:** SLF1 done.
- **Scope:** see `decisions/0008-erc8004-deployment.md`. Deploy `IdentityRegistry` + `ReputationRegistry` to Mantle Sepolia (chain 5003) via Foundry. Update `RegistryAddresses::mantle_testnet()` in `crates/xianvec-identity/src/client.rs`. Drop the `#[ignore]` on the integration tests; smoke a register + giveFeedback round-trip.
- **Why pulled forward:** ADR 0008 originally gated this on Phase 11.5 forward Orderly run. The pivot makes ERC-8004 a week-1 dependency. Mainnet still gated on Phase 9 eval clearing per ADR 0008.
- **Blocking:** YES for SLF3, SLF4, SLF5.

### SLF3. Mint per-strategy NFT on `ab_compare` startup

- **Trigger:** SLF2 done.
- **Scope:** extend `xianvec-eval::ab_compare` to call `IdentityClient::register` for each Strategy in the active set on run start, persisting `(strategy_name, agent_id, agent_uri)` mapping. `agent_uri` points to a stable manifest (code commit + Strategy adapter type + risk preset). Idempotent — re-runs reuse the existing `agent_id` if the manifest hash matches.
- **Decision:** each `VectorConfig` mode of TraderArm gets its own NFT (TraderArm-Off, -On, -Random, -Orth = four NFTs). The leaderboard view needs them as separate units.
- **Blocking:** YES for SLF4. Also supersedes the vectors_off/vectors_on placeholder manifests in F4.

### SLF4. Per-cycle Reputation Registry write path

- **Trigger:** SLF3 done.
- **Scope:** at end of each `ab_compare` cycle, sign + post a performance receipt to the Reputation Registry per strategy: `(value=cycle_pnl_bps, valueDecimals=4, tag1="cycle", tag2=cycle_id, endpoint=https://...full_metrics, feedbackHash=keccak(metrics_blob))`. `xianvec-identity::ReputationClient::give_feedback` already wired in ADR 0008 stub.
- **Riskiest seam.** Engine writes → Mantle Sepolia → dashboard reads back. Get end-to-end smoke green before scaling beyond one strategy / one cycle.
- **Blocking:** YES for SLF10 dashboard.

### SLF5. Validation Registry — signed-oracle backtest receipts

- **Trigger:** SLF4 done; held-out backtest cycle implemented (SLF9).
- **Scope:** add `ValidationRegistry` contract to ADR 0008's Foundry deployment. After each evening Karpathy cycle, post a signed-oracle receipt for each kept mutation: `(strategy_id, parent_strategy_id, holdout_window_id, sharpe_delta_bps, mutation_diff_hash)`. v1 uses operator-signed oracle, NOT TEE/zkML. v2 escalates to TEE attestation.
- **Why operator-signed and not TEE/zkML:** scope. TEE setup on Mantle is multi-week. Signed oracle is a credible verification-layer story for hackathon judges and matches the ERC-8004 EIP's "different trust models" framing — reputation systems vs validation, not strictly cryptographic.
- **Blocking:** non-blocking for headline demo, but materially stronger story with it.

### SLF6. Onchain strategy baselines — Nansen / funding / stablecoin / liquidation

(supersedes F14 — pulled forward from "deferred to post-headline" to "week 1 critical")

- **Trigger:** SLF1 done.
- **Scope:** four strategies as `Strategy` impls in `crates/xianvec-eval/src/baselines/onchain/`, each consuming `OnchainPanel` fields. Data sourcing: Nansen API key, funding-rate feed (Bybit), stablecoin exchange-flow feed, liquidation feed. See F14 for original strategy descriptions (smart-money copy, funding-rate fader, stablecoin exchange-inflow risk-off, liquidation cascade fader).
- **Why pulled forward:** the marketplace narrative is credibly Mantle-native only if the seed strategy population includes Mantle-native onchain signals. Without these, the loom is "AI mutates classical TA on DEX flow" — generic.
- **Blocking:** YES for the demo's "credibly Mantle-native" framing.

### SLF7. TA baselines — Bollinger / Donchian / Fibonacci / MA-triple

(supersedes F15 — pulled forward to "week 2 critical")

- **Trigger:** SLF6 in progress.
- **Scope:** four more `Strategy` impls in `crates/xianvec-eval/src/baselines/ta/`. Pure code work, no new data deps. See F15 for per-strategy parameters.
- **Why pulled forward:** part of the seed population. Loom needs ≥10 strategies to demonstrate selection-and-mutation visually by demo day.
- **Blocking:** non-blocking individually; collectively gating the genealogy hero-shot.

### SLF8. Strategy genealogy — `program.md` versioning + parent hash on chain

- **Trigger:** SLF3 done.
- **Scope:** each strategy variant has a `program.md` (Karpathy autoresearch unit-of-work). Mutations append to a content-addressed log with `(version, parent_hash, mutation_diff)`. The Identity Registry's `agentURI` for a forked strategy points to a manifest containing its `program.md` hash + parent `agent_id`, so the genealogy tree is reconstructable from on-chain state alone.
- **Blocking:** YES for SLF10 genealogy view.

### SLF9. Evening Karpathy loop — wrapper around `xianvec-intern`

- **Trigger:** SLF3, SLF4 done.
- **Scope:** new module `xianvec-eval::loom::evening_cycle`. Per strategy: read day's trade ledger + `program.md`, ask intern for one mutation, paper-test new variant against held-out window via `ab_compare`, accept (mint new NFT via SLF3, post Validation receipt via SLF5, fork lineage via SLF8) or reject (log to mutation registry as proposed-but-pruned). Bound mutations per night per strategy by `[loom] mutations_per_night = N` in config.
- **Decision:** start with constrained mutation surface — knob-level (size, stops, indicator parameters) + indicator selection — for safety + repeatability. Open up to free-form `program.md` editing in v2 once basic loop is stable.
- **Blocking:** YES for "self-improvement" claim in demo.

### SLF10. Next.js dashboard — live ladder + genealogy + risk-preset delegate

- **Trigger:** SLF4 done (reputation reads available); SLF8 done (genealogy reconstructable).
- **Scope:** new repo `xianvec-dashboard` (separate from Rust workspace). Next.js 15 + viem + wagmi. Three views:
  - **Ladder.** Live-updating list of strategies sorted by trailing performance (configurable window). Reads Reputation Registry directly.
  - **Genealogy.** Per-lineage tree of strategy variants, each node clickable for `program.md` diff vs parent + Validation receipt.
  - **Delegate.** Risk-preset selector → filtered strategy list → one-click delegation flow.
- **Auth/wallet:** account abstraction via Privy or Dynamic; one-click social login. Match EasyVault's UX bar.
- **Blocking:** YES for hackathon demo content.

### SLF11. Risk-preset configuration wired to `xianvec-risk`

- **Trigger:** SLF10 in progress.
- **Scope:** define `RiskPreset::{Conservative, Balanced, Aggressive}` in `xianvec-risk`. Each strategy NFT manifest declares which preset(s) it's compatible with. Dashboard's Delegate view filters on this. Tentative defaults (finalise week 2): Conservative = max 2% per position, no leverage, 30% max drawdown halt; Balanced = 5% / 1× / 50%; Aggressive = 10% / 2× / 70%.
- **Blocking:** non-blocking for demo (defaults work); blocking for the "trust" framing.

### SLF12. `control-vectors` cargo feature — pre-merge refactor

- **Trigger:** post-hackathon (after Jun 15 submission).
- **Scope:** lift the existing `default-members` opt-in pattern from `xianvec-identity` to a named feature `control-vectors`. Gate `xianvec-introspect` and `xianvec-inference`'s steering paths. CI runs both `cargo build` (default, no GPU) and `cargo build --workspace --features control-vectors`. Update `setup_runpod.sh` to install vectors-on dependencies only when the feature is selected.
- **Why deferred to post-hackathon:** during the sprint, hackathon work lives on `hackathon/turing` and CV work lives on `main`. The feature flag is the merge-back glue; doing it pre-sprint adds friction without payoff during the sprint.
- **Blocking:** YES for hackathon→main merge after Jun 15.

### SLF13. Cross-pollination — agents read other agents' Reputation

- **Trigger:** SLF9 stable; week 4+ if scope holds.
- **Scope:** before proposing a mutation, the intern reads the top-K performing agents' Reputation entries (incl. their `program.md` diffs) and incorporates them as priors. Converts ERC-8004 from "storage" into "learning substrate." Knob: `[loom] cross_pollination_weight = 0.0..1.0` (start own-history-dominant).
- **Blocking:** non-blocking. Cut to v2 narrative slide if week 4 is tight.

### SLF14. SMA(30) and SMA(90) on `IndicatorPanel`

(supersedes F17)

- **Trigger:** SLF6 / SLF7 in progress.
- **Scope:** push 30/90 SMA computation upstream from inline-in-MA-crossover-baseline into `xianvec-data::indicators` so multi-strategy lookups don't recompute. Add `sma_30: Option<f64>` and `sma_90: Option<f64>` to `IndicatorPanel`.
- **Blocking:** non-blocking; cosmetic.

### SLF15. Pluggable Trader stage — `TraderBackend` trait

(supersedes F23 — the Strategy Loom IS this)

- **Trigger:** post-hackathon, but the architecture should be designed during the sprint.
- **Scope:** see F23 for original framing. `Qwen3VectorTrader` (existing, behind `--features control-vectors`) and `McpAgentTrader` (new, default) become two `Strategy` impls among many on the marketplace ladder. F23's "vectors-on / vectors-off pairing breaks down" concern dissolves: in the marketplace world, every strategy is on its own ledger, no implicit pairing.
- **Blocking:** non-blocking for v1 hackathon; the loom can run with TraderArm-Off (DeepSeek-via-OpenAICompat per F24 short-term) as the only intern-driven strategy and onchain/TA baselines as the population.

### SLF16. Demo polish — pitch video, README, submission package

- **Trigger:** week 5.5.
- **Scope:** 90-second pitch video (loom in action + dashboard click-through + headline numbers); README in hackathon submission format (problem → solution → architecture → demo → judging-criteria mapping); submission package on DoraHacks at `dorahacks.io/hackathon/mantleturingtesthackathon2026`.
- **Blocking:** YES for actually submitting on Jun 15.

---

## Control Vector queue (CVF)

Personal-project track. Lives on `main` and (after SLF12) behind
`--features control-vectors`. Numbering preserved as historical anchors;
each entry tagged `[CVF]` in its title.

### F1 [CVF]. Phase 4.3 hard gate — re-run spike directional match through candle runtime

- **Trigger:** production Conviction vectors extracted via `tools/extract_vectors/extract_vectors.py` on a rented GPU.
- **Scope:** call `xianvec_inference::substrate::validate_directional_match(bundle, holdout, engine)` on the loaded production vector; assert ≥0.75 match rate (matches spike empirical PASS). Drop the `#[ignore]` on the integration test.
- **Blocking:** YES for Phase 9.3 headline run on the personal track. Non-blocking for hackathon submission (TraderArm-On not in default build per SLF12).

### F2 [CVF]. Extract production Conviction vector (and Patience/Risk/Trend pipeline-only)

- **Trigger:** GPU access (Vast.ai/RunPod) provisioned.
- **Scope:** run `python tools/extract_vectors/extract_vectors.py --model Qwen/Qwen3-32B --spec specs/conviction.yaml --layers 20,32,42,50 --out data/vectors/conviction_v1` plus the same for `patience.yaml`, `risk.yaml`, `trend.yaml`. Generate Random + Orthogonal control vectors against the Conviction axis. Verify all manifests parse cleanly via `xianvec_inference::substrate::load_vector`.
- **Blocking:** YES for personal-track Phase 9 (cannot A/B vectors-on/off without vectors-on). Non-blocking for hackathon.

### F3 [CVF]. Live Trader-as-Strategy adapter — **LANDED 2026-05-04**

- **Status:** landed at `crates/xianvec-eval/src/baselines/trader_arm.rs` with `VectorConfig::{Off, On, Random, Orthogonal}`. Only the `Off` arm is end-to-end tested (`cache_key_pairs_arms_for_same_setup_id`). The other three accept their config + compile but degrade-with-warn to vectors-off behaviour until F1 + F2 land — they emit a `tracing::warn` noting the directional claim is invalid pre-F2.
- **Side effects of F3:**
  - `Strategy::decide` lifted to `async` via `#[async_trait]`. All seven Phase 7 baselines + the in-test impls in the harness updated. Phase 8.2 harness `await`s `decide` at `harness.rs:232`; no other API changes.
  - `xianvec-eval` gains deps on `xianvec-trader`, `xianvec-intern`, `xianvec-inference` (per the FOLLOWUPS guidance — eval is the "things that implement Strategy" home).
  - Phase 9.1 + 9.2 (A/B orchestrator + `xvn ab-compare` runner) landed in the same session; see `crates/xianvec-eval/src/ab_compare.rs` and `crates/xianvec-cli/src/commands/ab_compare.rs`.
- **Blocked-on-F3 items now unblocked:** Phase 9.2 A/B runner (built), F16 vectors-OFF/RANDOM/ORTHOGONAL controls (compile path live; pending F1 + F2 for directional validity).

### F9 [CVF]. Measure `target-cpu=native` numeric delta vs default codegen

- **Trigger:** stable bench rig (controlled thermal state, repeated trials with statistics).
- **Scope:** rerun smoke-qwen3 with and without `RUSTFLAGS="-C target-cpu=native"`, ≥10 trials each, report median + p95 decode/prefill tok/s. The current ADR 0007 §"Re-measured" cites 5–16 tok/s with 3.2× variance across 5 runs — that's not enough to commit to a single number. From `decisions/0007-inference-throughput-routes.md`.
- **Blocking:** non-blocking; numbers are advisory until forward paper exposes whether warm cache holds.

### F10 [CVF]. Codify `RUSTFLAGS=-C target-cpu=native` in `.cargo/config.toml`

- **Trigger:** F9 confirms the win is material (≥1.5×) and stable.
- **Scope:** add `[build] rustflags = ["-C", "target-cpu=native"]` so contributors don't have to remember the flag. From ADR 0007 recommendation.
- **Blocking:** non-blocking.

### F11 [CVF]. Shader pre-warm pass during engine init

- **Trigger:** cold-start latency materially affects the forward-paper experience (a 17–90 s wait per process start).
- **Scope:** `Qwen3Engine::new` runs a discard 1-token forward pass on a fixed prompt shape before returning, so the first user-visible prefill doesn't pay the JIT. From ADR 0007 v1 cold-start workaround.
- **Blocking:** YES for personal-track Phase 11.1 if cold-start latency is unacceptable to the operator; non-blocking otherwise.

### F12 [CVF]. mlx-rs viability spike (ADR 0007 option B)

- **Trigger:** F11 isn't enough OR the rented-GPU run (Phase 9.3) shows the same shader-JIT pattern at higher precision.
- **Scope:** 1–2 day spike: run `oxideai/mlx-rs` against the same Q4 weights; record TTFT + decode tok/s; verify `mlx-rs` exposes (or can be extended to expose) a pre-/post-block forward hook. From `decisions/0007-...` proposed plan.
- **Blocking:** conditionally blocking on cold-start.

### F13 [CVF]. Phase 8.5 boundary probes (Glamin pattern formalization)

- **Trigger:** Phase 9.2 A/B runner stable; need a regression-detection net for vector / prompt / model changes.
- **Scope:** curated `data/probes/` corpus (ambiguous regime transitions, low-liquidity setups, hardest historical decisions, flash-crash conditions, regulatory edge cases). `ProbeRunner` in `xianvec-eval`; `IntrospectionHook`-attached when `--introspect` set. From implementation-plan §8.5.
- **Blocking:** non-blocking for v1 demo; recommended before Phase 11 forward paper.

### F16 [CVF]. Vectors-OFF / RANDOM / ORTHOGONAL experimental controls

- **Trigger:** F2 (extracted vectors) + F3 (Trader-as-Strategy adapter, landed).
- **Scope:** the experimental "control" arms in Phase 9.2 A/B runner — three more Strategy adapters that wrap the Trader with each `VectorConfig`. Reuses the same Trader + Intern; differs only in which vector bundle is loaded.
- **Blocking:** YES for personal-track Phase 9.2 if the headline experiment depends on the Random/Orthogonal nulls. Non-blocking if a vectors-OFF vs vectors-ON two-arm comparison is acceptable for v1.

### F26 [CVF]. Split Mac/MLX path out of the shared `scripts/download_qwen.py`

- **Trigger:** any time after the current GPU headline run completes — non-blocking; the recent `setup_runpod.sh` torch-wheel fix already made the Linux path MLX-clean.
- **Current state:** `scripts/download_qwen.py` is dual-purpose — Step 1 grabs `mlx-community/Qwen3-32B-4bit` (~18 GB, Mac/Apple Silicon spike), Step 2 grabs GGUF Q4. Mac operators always get GGUF too; Linux operators have no reason to call this script at all (setup_runpod.sh does its own download). `tools/extract_vectors/spike/{extract,validate}.py` imports `mlx`/`mlx_lm` — fine on Mac, hard-fails on Linux even though they live under the otherwise cross-platform `tools/extract_vectors/` tree.
- **Scope:**
  - Rename `scripts/download_qwen.py` → `scripts/download_qwen_mlx.py`; strip the GGUF half (Linux/CUDA users go through `setup_runpod.sh`).
  - Add `scripts/setup_mac.sh` — Apple Silicon counterpart to `setup_runpod.sh`. Preflight (`uname -sm` → Darwin/arm64 check), venv, `pip install mlx mlx-lm transformers accelerate repeng pyyaml numpy`, call `download_qwen_mlx.py`, print next steps for the Phase 0.3 spike. Mirror the stage layout (preflight → python → hf → model → verify) so muscle memory transfers.
  - Add a clearer "Apple Silicon only — requires `mlx`/`mlx-lm`" docstring header to `tools/extract_vectors/spike/extract.py` and `validate.py` so they don't read as cross-platform. Optionally move them under `tools/extract_vectors/spike/mlx/`.
  - MANUAL.md M0/M1: split into "Linux GPU box (RunPod / Vast.ai) → `scripts/setup_runpod.sh`" vs "Apple Silicon (local dev / spike) → `scripts/setup_mac.sh`".
- **Blocking:** non-blocking. Refactor for clarity; current Linux path is already correct.

### F27 [CVF]. Python eval on Qwen3.6 — does the fear-greed vector actually steer hybrid-arch outputs?

- **Trigger:** Qwen3.6 weights on disk + fear-greed vector extracted (both done 2026-05-05). This is the immediate next step.
- **Scope:** mirror `tools/extract_vectors/spike/validate.py`'s structure against Qwen3.6 fp16 via HF `transformers` (NOT MLX — MLX's qwen35 path is also unproven). 20+ holdout prompts × magnitudes [-2, -1, 0, +1, +2] × the layer where extraction happened. Greedy decode, log per-prompt logit-difference, mean disposition score swing, residual-norm shift, and a coherence/safety control set. Compare against ADR 0002's 1.17 swing baseline (toy axis on Qwen3-32B Q4) — Qwen3.6 is RL-trained so expect ~½× per-layer logit-diff. Bias the layer choice to L36–L46 (RL models peak at 70–85% depth, not the L42 used on Qwen3-32B).
  - Pass: any clear monotonic swing across α with zero coherence violations. Treat the strict 8-criterion gate from ADR 0002 as advisory, same substantive PASS framing.
  - Fail: vectors do not transfer through the DeltaNet recurrence in any meaningful way → ADR 0009 closes as "Qwen3-32B remains production through v1," and the Qwen3.6 weights become exploration artifacts only.
- **Blocking:** YES for ADR 0009 ratification. Non-blocking for hackathon. From `decisions/0009-qwen3-next-runtime-options.md`.

### F28 [CVF]. llama.cpp `--control-vector` spike on Qwen3.6 GGUF

- **Trigger:** F27 PASSes.
- **Scope:** generate a Qwen3.6 GGUF (jukofyork/control-vectors tool or hand-export from F27's vector); run `llama-cli --model qwen3.6-q4_k_m.gguf --control-vector <fear-greed>.gguf -p <fixed prompt>` at α ∈ [-2, +2]. Compare behavioural outcome to F27's Python results. Pass: same directional shift in disposition / fear-greed scoring at comparable α. Fail: llama.cpp's cvec hook point (post-FFN-residual, before next-layer-norm — `qwen3next.cpp:164`) does not propagate through DeltaNet's recurrent state the way Python's per-token activation injection does.
  - This is the validation oracle for the runtime question. With both F27 (Python ground truth) and F28 (llama.cpp ground truth) in hand, ADR 0009's δ vs γ choice has two independent references to validate against.
- **Blocking:** YES for ADR 0009 ratification. Non-blocking for hackathon. From `decisions/0009-qwen3-next-runtime-options.md`.

### F29 [CVF]. Candle DeltaNet port — chunked variant

- **Trigger:** F27 PASS + F28 PASS + ADR 0009 ratifies route δ.
- **Scope:** port `build_layer_attn_linear()` from llama.cpp `src/models/qwen3next.cpp` (lines 367–550) and the chunked variant of `delta-net-base.cpp` to a new `crates/xianvec-inference/src/model/qwen3next_steered.rs`. The chunked variant uses `cumsum`, `solve_tri`, and standard matmul/RMSNorm — all already in candle, so no custom Metal/CUDA kernels needed for the port. Add cvec hook as a one-line `LayerHook::apply()` after the FFN residual (matches llama.cpp's hook point in `qwen3next.cpp:164`).
  - Estimate: ~250–350 lines for the DeltaNet block, ~80–100 for tensor/hparam loading, ~10 for cvec hook. ~1–2 weeks calendar.
  - Validation: layer-by-layer residual parity vs F27 (Python) and F28 (llama.cpp) — both ground truths. Single-token forward parity first, then multi-token, then logit parity, then behavioural cvec parity at α-sweep.
- **Blocking:** YES for adopting Qwen3.6 in production on the personal track. Non-blocking for hackathon.

### F30 [CVF]. Verify repeng layer-discovery against Qwen3.6 / `qwen35` arch string

- **Trigger:** before F27. Should be ~30 minutes; cheap to do upfront.
- **Scope:** load Qwen3.6-27B in HF `transformers`, instantiate `repeng.ControlModel`, and confirm `model_layer_list()` returns the expected layer count without an explicit override. PR #73 (Sep 2025) fixed `attention_type` AttributeError on Qwen3 by adding `__getattr__` delegation, but Qwen3.6 declares architecture `qwen35` — verify that the layer walk (`model.model.layers` first, then fallbacks) hits cleanly. If not, add `model.repeng_layers = list(model.model.layers)` as the override. Document the resolved path in F27's eval script.
- **Blocking:** non-blocking; defensive.

### F31 [CVF]. Watch upstream candle for qwen3-next / DeltaNet PRs

- **Trigger:** ongoing (weekly check until F29 lands or v1 ships).
- **Scope:** subscribe to `huggingface/candle` PRs filtered for `qwen3`, `mamba`, `deltanet`, `state-space`, `linear-attention`, `hybrid`. If a clean upstream impl lands before F29 starts, F29 collapses to "use upstream + add cvec hook" — saves 1–2 weeks. If it lands during F29, decide whether to abandon the in-flight port or contribute it back. As of 2026-05-05 there are zero in-flight PRs.
- **Blocking:** non-blocking.

### F32 [CVF]. Tier-2 model swap eval — DeepSeek-R1-Distill-Qwen-32B

- **Trigger:** Qwen3-32B headline run (Phase 9.3) lands, AND the eval suggests steering headroom is the binding constraint on the result (e.g., per-layer logit-difference materially below the Mitra/Subramani published values for Mistral-7B-class models, or vector strength saturates before behavioural change is decisive). Also valid trigger: F27 fails (Qwen3.6 hybrid path closes), so Qwen3-32B is locked in but you want to push capability further before forward paper.
- **Why this model specifically:** four properties stack favourably — (1) **distilled** from R1 rather than RL-aligned, so per Omar Ayyub's 2026 layer-sweep finding it should produce ~2× the per-layer logit-difference of Qwen3-32B for the same vector; (2) **Qwen2.5 architecture** (the base it was distilled onto), which candle supports via the `quantized_qwen2` path — vendoring is shorter than the existing Qwen3 work because Qwen2.5 attention is simpler (no q/k norm layers Qwen3 added on top); (3) **reasoning-capable** from the R1 distillation, which directly improves the Trader stage's structured-judgment task without adding an agent loop; (4) **dense, pure transformer, 32B** — fits the same 36 GB Q4 budget as the current production model, no MoE/hybrid/SSM compatibility concerns.
- **Scope:**
  - Vendor `candle_transformers::models::quantized_qwen2` to a new `crates/xianvec-inference/src/model/qwen2_steered.rs` mirroring the `vendor_qwen3.rs` pattern (LayerHook contract, no other API change). Estimated ~600–800 LoC (smaller than vendor_qwen3 because Qwen2.5 has fewer normalization layers per block).
  - Re-run the F27 / ADR 0002 vector-extraction + directional-match flow against `deepseek-ai/DeepSeek-R1-Distill-Qwen-32B` at fp16 (extract) and Q4_K_M (runtime). Measure per-layer logit-difference at the optimal layer (likely L36–L46 — distilled models peak around 50–65% depth per Omar Ayyub vs RL's 70–85%, so try L32 first).
  - Compare against the Qwen3-32B Q4 numbers from ADR 0002 / Phase 4.3 hard gate. **Pass condition:** per-layer logit-difference at least 1.5× the Qwen3-32B baseline at comparable α, with no degradation in coherence-violation rate. **Bonus signal:** Trader-stage briefing quality improves on hard probe sets (F13 boundary probes if landed).
  - Decision point: if PASS, ratify in ADR 0011 as the new production model and migrate Phase 4 + Phase 9 work to the qwen2_steered path. If FAIL (steering doesn't materially improve OR coherence regresses), keep Qwen3-32B and document the negative result.
- **Why "Tier 2" not "Tier 1":** different question from ADR 0009's F27–F31 lane. F27–F31 ask "can we adopt the new architecture?" F32 asks "should we adopt a different *training method* on the architecture we already support?" Both lanes can run in parallel — they don't conflict.
- **Why not just go to QwQ-32B or Qwen2.5-32B-Instruct instead:** QwQ is a reasonable substitute (same Qwen2.5 base, distilled approach, slightly weaker reasoning per benchmarks but more conventional alignment) — worth running as a comparison arm in F32 once the runtime path is open. Vanilla Qwen2.5-32B-Instruct gives less RL alignment than Qwen3 but no reasoning lift, which is the trade DeepSeek-R1-Distill-Qwen-32B avoids.
- **Estimate:** ~3–5 days for the candle vendoring + extraction tooling adjustment + first eval pass. Validation against Qwen3-32B baseline is another 1–2 days.
- **Blocking:** non-blocking. v1 headline still wants the validated Qwen3-32B path. F32 is post-headline capability lift.
- **References:**
  - [Omar Ayyub — What I Learned (And Didn't) Steering Qwen3 Models](https://omar.bet/2026/01/17/What-I-Learned-Steering-Qwen3-Models/) (distilled vs RL layer-depth + headroom finding)
  - [DeepSeek-R1-Distill-Qwen-32B model card](https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B)
  - [QwQ-32B model card](https://huggingface.co/Qwen/QwQ-32B) (comparison arm)
  - `decisions/0001-inference-backend.md` (architecture support patterns to mirror)
  - `decisions/0002-spike-validation.md` (validation methodology to reuse)
  - `decisions/0009-qwen3-next-runtime-options.md` (sibling lane — F27–F31)

---

## Shared queue

Infrastructure used by both tracks. Lives on `main`.

### F4 [SLF — superseded by SLF3, SLF4]. ERC-8004 manifests for both arms + harness wiring (runtime-optional)

- **Status:** Original framing was "vectors_off / vectors_on manifests for the personal-track A/B run." Pivot reframes as "per-strategy NFTs across the marketplace population." See SLF3 (mint per-strategy NFT on `ab_compare` startup) + SLF4 (per-cycle Reputation write path).
- **Original scope (preserved for reference):** Phase 6.5 already shipped placeholder `identity/vectors_{off,on}.agent.json` with `code_commit=PENDING`, `contact=PENDING`, and (for vectors_on) `manifest_hashes=["PENDING_PHASE_4_2_EXTRACTION"]`. Before the forward run, fill these from `git rev-parse HEAD` and the actual production vector manifest hashes from F2; mint via `IdentityClient::register` on Mantle testnet first, mainnet after Phase 9 eval clears.
- **What still applies post-pivot:** the runtime-optional gating (`identity.enabled = true/false` in `config/default.toml`) carries forward — the harness must run without Mantle credentials when identity is disabled, and `xianvec-identity` stays an opt-in workspace member.
- **Blocking:** the *concept* is now in SLF3/SLF4. The original placeholders can be deleted post-SLF3.

### F5 [Shared]. Orderly testnet credentials + smoke trade

- **Trigger:** Phase 6.3 lands.
- **Scope:** complete brokered onboarding once (`xvn setup --orderly-onboard` per plan §6.3); store `(orderly_key, orderly_secret, orderly_account_id)` in `op` (1Password); place + cancel a small `PERP_BTC_USDC` order against testnet to validate the full path. SDK errors mapped to `ExecutorError`.
- **Blocking:** YES for Phase 11.5 (personal track) and forward delegate-flow demo (hackathon).

### F6 [Shared]. `setup_id` reuse guard in the harness

- **Trigger:** Phase 9.1 ops crate work.
- **Scope:** harness rejects setups whose `setup_id` was already cached this run; cache key is `(setup_id, intern_provider, intern_model)` per Tier 1 fix #1. From `decisions/0005-lookahead-audit.md` follow-up #1.
- **Blocking:** non-blocking; defensive.

### F7 [Shared]. Lookahead-bias boundary-condition test

- **Trigger:** Phase 9.1 ops.
- **Scope:** unit test that constructs a `MarketSnapshot` whose `recent_bars.last().timestamp` is *after* `snapshot.timestamp` (an impossible state); harness should reject the snapshot rather than process it. From `decisions/0005-lookahead-audit.md` follow-up #2.
- **Blocking:** non-blocking; defensive.

### F8 [Shared]. Document `MarketSnapshot` invariants

- **Trigger:** Phase 9.1 ops.
- **Scope:** doc comment on `xianvec-core::market::MarketSnapshot` listing the temporal invariants (recent_bars.last().timestamp ≤ snapshot.timestamp; recent_bars chronologically ordered; horizon_hours non-negative). From `decisions/0005-lookahead-audit.md` follow-up #3.
- **Blocking:** non-blocking; documentation hygiene.

### F18 [Shared]. Add `asset: AssetSymbol` to `TraderDecision` (resolves choices #1, #4 in `strategy-choices.md`)

- **Trigger:** multi-asset enabled in `whitelist.toml` (post-headline / post-hackathon).
- **Scope:** schema field add + cascade through xianvec-trader (prompt schema), xianvec-intern (briefing format), xianvec-risk (drop the separate `asset` parameter), xianvec-execution (Alpaca + Orderly stop pinning to BTC), xianvec-eval (drop `BacktestConfig.instrument`). Mechanical but wide.
- **Blocking:** YES for multi-asset.

### F19 [Shared]. Re-adopt `orderly-connector-rs` SDK when its `zeroize` pin loosens

- **Trigger:** `orderly-connector-rs` releases a version that no longer transitively pins `zeroize = "=1.3.0"` (currently 0.4.15 does, via `solana-sdk` → `ed25519-dalek 1.x`). The pin conflicts with `rustls 0.23` (workspace `reqwest 0.13`'s TLS) which needs `zeroize ≥ 1.7`.
- **Current state:** Phase 6.3 reimplements the five required Orderly REST endpoints directly via signed `reqwest` + `ed25519-dalek 2.x` calls. Signing scheme is byte-identical to the SDK's `auth::generate_signature` (Ed25519 over `${ts}${METHOD}${path}${body}`, base64-encoded, secret base58). Tests cover the path; ergonomics of the SDK are gone.
- **Scope:** swap the in-house REST shims for SDK calls (`OrderlyService::create_order`, `create_algo_order`, `cancel_order`, `get_account_info`, `get_positions`, `get_futures_info`). Keep the `OrderlyApi` trait so tests stay independent. Strip the local signing code.
- **Blocking:** non-blocking; current implementation is functional.

### F20 [Shared]. Upstream PR: gate Solana stack in `orderly-connector-rs` behind a feature

- **Trigger:** any time before F19's re-adoption (or never, if Orderly upstream fixes it without our PR).
- **Current state:** F19 documents the workspace-side workaround. The conflict is *not* workspace-specific — `orderly-connector-rs 0.4.15` has no `[features]` section, hard-pulls `solana-sdk = "=1.16.13"` + `solana-client = "=1.16.13"` + `ed25519-dalek 1.0` + `zeroize = "=1.3.0"` even for EVM-only users (the only consumer surface that actually exists for Mantle v1). Anyone in the modern async/rustls Rust ecosystem hits it.
- **Scope:** PR against `ranger-finance/orderly-connector-rs` adding:
  - `[features] default = ["solana", "evm"]` to preserve current behavior.
  - `solana-sdk`/`solana-client`/`solana_vault_cpi` and `ed25519-dalek 1.x` made `optional = true`, gated behind `feature = "solana"`.
  - For the `evm` feature, depend on `ed25519-dalek 2.x` (no zeroize pin); the EVM gateway's Ed25519 signing scheme works under either major.
  - Drop the `zeroize = "=1.3.0"` exact pin; let cargo resolve it.
- **Impact if landed upstream:** F19 collapses to "switch from in-house REST shims to `OrderlyService` calls behind `default-features = false, features = ["evm"]`." ~30–50 LoC PR upstream; tests should cover both `--features solana` and `--features evm` invocations.
- **Blocking:** non-blocking. Worth filing whether or not we want to take F19 ourselves; the wider Rust EVM ecosystem benefits.

### F21 [Shared]. Replace HTTP-backend Intern with an OpenClaw / ACPX agent-harness backend  *(partial — ACPX subprocess backend landed)*

**Landed 2026-05-04:** `AcpxIntern` in `crates/xianvec-intern/src/backend.rs` spawns `acpx <agent> exec --file -` (or `acpx --agent "<cmd>" exec --file -` in escape-hatch mode) with a wall-clock timeout, captures stdout, strips ACP markers (`[thinking]/[tool]/[done]`), and runs the result through the shared `parse_llm_response`. Wired into `xvn run-setup` and `xvn ab-compare` via provider strings `acpx` or `acpx:<agent>`. Setup script (`scripts/setup_runpod.sh`) installs Node + acpx and exposes the full ACPX built-in registry (claude / codex / gemini / opencode / cursor / copilot / qwen / kimi / iflow / trae / qoder / kilocode / kiro / droid / openclaw / pi) plus an escape-hatch slot for Hermes Agent — itself an ACP server, reached via `acpx --agent "hermes acp" exec ...`. The underlying agent CLI is NOT auto-installed; auth flows vary.

Hermes Agent (NousResearch) is the OpenClaw successor — its own README documents `hermes claw migrate` from OpenClaw — and it has direct first-class routes to Xiaomi MiMo / Kimi / GLM / MiniMax / Nous Portal that none of the other ACPX agents offer in one place. Because it ships an ACP adapter (`acp_adapter/` in the repo), no separate Rust backend is needed: `XVN_INTERN_ACPX_CUSTOM_CMD="hermes acp"` routes through the same `AcpxIntern` code path.

**Tools (landed 2026-05-04):** new crate `crates/xianvec-mcp/` ships a stdio MCP server (`xvn-mcp`) wrapping `xianvec-data` indicators as agent-callable tools — `xvn_rsi`, `xvn_sma`, `xvn_ema`, `xvn_bollinger`, `xvn_atr`, `xvn_macd`, `xvn_donchian`, `xvn_fib_retracements`, plus `xvn_health`. Built on rmcp 1.6 (the official Rust MCP SDK) so the wire contract is maintained upstream. The setup script writes `<acpx-workspace>/acpx.config.json` registering xvn-mcp as a stdio MCP server, and ACPX threads `mcpServers: [...]` into every agent session — so Hermes, Claude Code, Codex, OpenCode, and any future ACPX agent inherit the tools without further wiring. Pure compute, stateless, no data root or API keys; preserves backtest pairing because the agent supplies the input series from prompt context. Live API tools (funding rates, onchain panel reads) are deferred until the live data path is solid.

**Still open:** budget/cost telemetry, deterministic-fallback wiring (caller currently falls back manually by switching provider), live-data MCP tools (funding/onchain) once the data layer stabilises, backtest determinism story for agent-harness paths.

- **Trigger:** Phase 9 result is positive and we want to push the Intern's analytical depth before forward paper, OR Phase 11 forward run shows the Intern is the bottleneck on hard setups. SLF9 (evening Karpathy loop) is a major new caller of this path on the hackathon track.
- **Current state:** Phase 2.2 ships `OpenAICompatIntern` and `AnthropicIntern` — both single-shot LLM calls that take a prompt and emit `InternBriefing`. The backend trait surface is interchangeable by design (Tier 1 fix #1 + plan §2.2), so a new backend impl plugs in cleanly without touching the prompt builder, cache, or trader.
- **Open questions to resolve:** harness choice (pinned upstream vs thin home-rolled loop); whether the harness calls out to `xianvec-data` for indicator recomputation; cost / latency profile vs single-shot (5–10× wall time and token spend possible — need a budget cap and a fallback to single-shot when budget is hit); determinism for backtest (Tier 1 fix #2) — agent loops with tool use are inherently non-deterministic unless temperature=0 *and* all tool calls are deterministic. Backtest may have to use the simpler single-shot backend even after this lands.
- **Blocking:** non-blocking; pure capability lift. The current single-shot Intern is sufficient for the v1 headline result and for SLF9's evening cycle (proposing one mutation per night does not need an agent loop).

### F22 [Shared]. Add `VetoReason::TakeProfitTooTight` (resolves choice #2 in `strategy-choices.md`)

- **Trigger:** any other `VetoReason::Custom(...)` site lands in the codebase.
- **Scope:** one line in `xianvec-core::trading.rs` enum + serde rename + cascade through any exhaustive `match VetoReason {...}` — `xianvec-risk::rules::take_profit_rr` switches off `Custom("rr_too_low")`.
- **Blocking:** non-blocking; quality-of-enum.

### F24 [Shared]. DeepSeek-TUI as a reasoning intern — short-term via OpenAI-compat, long-term via Hmbown cargo mirror

- **Trigger:** want DeepSeek's reasoner (R1) or chat (V3.x) line in the Stage 1 Intern slot.
- **Short-term (no code):** DeepSeek's hosted API is OpenAI Chat Completions wire-compatible. Use the existing `OpenAICompatIntern` against `https://api.deepseek.com/v1` with `DEEPSEEK_API_KEY`; `deepseek-reasoner` emits `<think>...</think>` blocks which `strip_reasoning` (`crates/xianvec-intern/src/reasoning.rs`) already handles. Single-shot, deterministic at `temperature=0` — the *right* shape for Stage 1 (briefing only, no tool use), and unlike `AcpxIntern` it pairs cleanly for backtest (Tier 1 fix #1). No new backend needed. **This is the default intern path for the hackathon submission** — see SLF15 / TraderArm-Off.
- **Long-term (release-time note):** there's a Cargo-native rewrite/mirror of DeepSeek-TUI at https://github.com/Hmbown/DeepSeek-TUI (Hmbown fork). At release time, mention it in our README and consider shipping a zh-CN README localization pointing zh-CN users at that fork (and at Hermes Agent → Xiaomi MiMo / Kimi / GLM / MiniMax routes via ACPX) — the audience for a Rust-first DeepSeek harness skews heavily zh-CN.
- **What we'd actually have to build to drive DeepSeek-TUI as an *agent* (not just the API):** either (a) ~2–3 days for an external `deepseek-tui-acp-shim` binary that translates ACP ↔ DeepSeek-TUI's existing one-shot mode (plugged in via `XVN_INTERN_ACPX_CUSTOM_CMD`), or (b) ~5–10 days upstreaming an `acp` subcommand into DeepSeek-TUI itself. Skip both unless the agent loop (file I/O, multi-step tool use) starts paying for itself in briefing quality — for Stage 1 it doesn't.
- **Blocking:** non-blocking. Short-term path is zero-code.

### F25 [Shared]. Author a `xianvec` Claude Code skill

- **Trigger:** after the GPU headline run lands and the operator surface stops moving every other session. Post-hackathon is also a natural trigger — the SLF surface is fresh tribal knowledge worth capturing.
- **Scope:** package the project's tribal knowledge as a skill so a fresh Claude Code session ramps without grepping. Likely contents:
  - **Setup & ops** — `scripts/setup_runpod.sh` stage map, env-var contract (`.env.local` keys, `XVN_INTERN_*`, `XVN_MODEL_*`), how to resume a half-finished install via `ONLY=<stage>`, the torch/CUDA driver-version pitfalls (cu126 vs cu128), Q4/Q5/Q6/Q8/fp16 selection rationale.
  - **Vectors** — extraction recipe (`tools/extract_vectors/extract_vectors.py` flags, `--out` is a path-prefix, random + orthogonal controls auto-emit), manifest schema, `xvn explain-vectors` for verification, layer choices (20/32/42/50 for Qwen3-32B), conviction/patience/risk/trend axes.
  - **Strategies / arms** — `Strategy` trait surface (`async_trait`-lifted), `TraderArm` with `VectorConfig::{Off, On, Random, Orthogonal}`, where new baselines plug in (`crates/xianvec-eval/src/baselines/`), how the A/B harness pairs cache keys per `setup_id` (Tier 1 fix #1).
  - **Loom (post-hackathon)** — SLF1–16 outcomes: ERC-8004 mint flow, evening cycle, dashboard data shape.
  - **Intern backends** — when to pick `OpenAICompatIntern` (deterministic, backtest-safe) vs `AnthropicIntern` vs `AcpxIntern` (agentic, forward-paper only) vs the F24 deepseek-via-openrouter path; how to add a new backend.
  - **MCP tool surface** — `xvn-mcp` tools (rsi/sma/ema/macd/bollinger/atr/donchian/fib/health), how `acpx.config.json` advertises them, what's intentionally NOT a tool (live-data — preserves backtest pairing).
  - **Monitoring / reports** — `docs/dashboard.md`, `xvn show-metrics` / `xvn show-decision` / `xvn report`, where reports land (`reports/headline_Q8/<date>.{json,md}`).
  - **Phase-map cheat sheet** — current state of phases 9 / 10 / 11 plus FOLLOWUPS Fn-codes / SLFn-codes so the assistant knows what's blocking what.
  - **Don'ts** — never recommend `AcpxIntern` for backtest pairing; never mock the real DB in integration tests; never commit the unbundled torch wheel back into requirements.txt.
- **Format:** YAML-frontmatter skill under `~/.claude/skills/xianvec/` (name, description, triggers) + a body with the cheat sheet + `references/` for longer per-area pages (vectors, intern, mcp, ops, loom). Description must be specific enough that the loader picks it up only on xianvec sessions, not every Rust project.
- **Validation:** dry-run a fresh session with `/<task>` against the skill — "extract a conviction vector at layer 32 for Qwen3-32B" should produce the right command without me having to re-explain `--out`-is-a-prefix.
- **Open questions:**
  - User-installable vs project-local (`.claude/skills/xianvec/` checked in)? Project-local survives across machines + onboards collaborators; user-installable stays light. Probably both — minimal user skill that points at the project copy.
  - Auto-trigger heuristics: filename patterns (`crates/xianvec-*/`), workspace-root marker (`Cargo.toml` containing `xianvec-core`)? Description-based discovery is usually enough.
- **Blocking:** non-blocking. Quality-of-life for future sessions; deferred until phase 9 headline + GPU experiment land OR until post-hackathon merge so the contents stop churning.
