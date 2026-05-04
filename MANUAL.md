# Manual operator tasks

Things that **cannot** be done from inside Claude Code / cargo / a clean repo.
Each entry: trigger, what's needed, exit criterion, FOLLOWUPS cross-ref.

Sorted by which milestone they unblock. Keep this file in sync with
`FOLLOWUPS.md` — that file tracks engineering follow-ups; this one tracks
operator-side prerequisites.

---

## Tier 1 — blocking the Phase 9.3 headline backtest run

### M1. Provision GPU (Vast.ai or RunPod)

- **Trigger:** ready to extract production vectors + run the headline backtest.
- **What:** rent an A40 / A100 / H100 spot. Approximate burn: ~$0.40–1.50/hr.
  The full vector-extraction + backtest cycle fits in 2–4 GPU-hours; budget
  $5–20 plus iteration.
- **Setup steps:**
  1. Create account on Vast.ai or RunPod.
  2. Pick an instance with ≥80 GB VRAM (A100/H100) or ≥48 GB (A40 — works for
     Q4 + bf16 single-layer extraction).
  3. SSH in; clone the repo on the box.
  4. `pip install -r tools/extract_vectors/requirements.txt` (huggingface_hub,
     mlx-lm or transformers, numpy, pyyaml).
  5. `huggingface-cli login` if you don't have the Qwen3-32B weights cached on
     the GPU.
- **Exit:** GPU box can `python tools/extract_vectors/extract_vectors.py --help`
  without crashing on imports.
- **Unblocks:** F1, F2, Phase 9.3.

### M2. Extract production Conviction vector + Patience/Risk/Trend pipeline-only

- **Trigger:** M1 complete.
- **What:** run `tools/extract_vectors/extract_vectors.py` for each of the four
  axes against `Qwen/Qwen3-32B`, layers 20/32/42/50.
- **Commands** (on the GPU box):

  ```bash
  python tools/extract_vectors/extract_vectors.py \
    --model Qwen/Qwen3-32B \
    --spec specs/conviction.yaml \
    --layers 20,32,42,50 \
    --out data/vectors/conviction_v1
  ```

  Repeat for `patience.yaml`, `risk.yaml`, `trend.yaml` (each: pipeline-only;
  Conviction is the active axis for v1).

  Then generate **Random + Orthogonal control vectors** against the
  Conviction axis (for the Phase 9.2 A/B nulls):

  ```bash
  python tools/extract_vectors/extract_vectors.py \
    --model Qwen/Qwen3-32B \
    --control random   --against data/vectors/conviction_v1 \
    --out data/vectors/control_random_v1
  python tools/extract_vectors/extract_vectors.py \
    --model Qwen/Qwen3-32B \
    --control orthogonal --against data/vectors/conviction_v1 \
    --out data/vectors/control_orthogonal_v1
  ```
- **Verify:**
  ```bash
  cargo test -p xianvec-inference substrate::tests::load_spike_fixture_returns_correct_shape
  ```
  passes against the new `.npz` + sidecar; manifest fields (`model_id`,
  `model_quant`, `contrast_pair_set_hash`) match what the runtime expects.
- **Then SCP back:**
  ```bash
  rsync -avz <gpu_user>@<gpu_host>:~/xianvec/data/vectors/ data/vectors/
  ```
- **Exit:** four `data/vectors/*_v1/` directories present locally; each loads
  via `xianvec_inference::substrate::load_vector`.
- **Unblocks:** F2, F1 (next), F16, Phase 9.2 / 9.3.

### M3. Re-run spike directional match through the candle runtime (F1 hard gate)

- **Trigger:** M2 complete.
- **What:** drop the `#[ignore]` on
  `crates/xianvec-inference/src/substrate.rs::tests::validate_directional_match_production`,
  load the production Conviction vector, run 5 holdout prompts steered, assert
  `directional_match_rate >= 0.75` (matches the spike's empirical PASS).
- **Run:**
  ```bash
  cargo test -p xianvec-inference \
    substrate::tests::validate_directional_match_production -- --ignored
  ```
- **Exit:** test passes; remove `#[ignore]`.
- **Unblocks:** F1, F3 directional validity, the headline run.
- **FOLLOWUPS:** F1.

### M4. Run the headline backtest at higher precision (Phase 9.3)

- **Trigger:** M3 passes.
- **What:** on the GPU box, run `xvn ab-compare` against the same setups +
  bars locally tested, but with the Q8_0 (preferred) or bf16 GGUF.
- **Command:**
  ```bash
  xvn ab-compare \
    --setups data/setups/2022_2024_paired.parquet.json \
    --bars data/bars/btc_2022_2024.json \
    --asset BTC \
    --arms off,on:data/vectors/conviction_v1/conviction.npz:data/vectors/conviction_v1/conviction.manifest.json:1.0,random:layer=20:dim=5120:alpha=1.0:seed=42,orthogonal:axis=conviction:path=data/vectors/control_orthogonal_v1/orth.npz:alpha=1.0 \
    --model models/qwen3-32b-Q8_0/qwen3-32b.Q8_0.gguf \
    --tokenizer models/qwen3-32b-Q8_0/tokenizer.json \
    --output reports/headline_Q8/$(date +%F).json
  xvn report --input reports/headline_Q8/$(date +%F).json --output reports/headline_Q8/$(date +%F).md
  ```
- **Exit:** `reports/headline_Q8/<date>.md` rendered with Δ-Sharpe + 95% CI
  for ≥100 paired trades on BTC-USD.
- **Unblocks:** Phase 12 self-review checklist; v1 demo headline.

---

## Tier 2 — blocking forward-paper / on-chain (Phase 11)

### M5. Set up Alpaca paper account + creds (F5 alpha)

- **Trigger:** ready to start Phase 11.1.
- **What:**
  1. Sign up at <https://alpaca.markets>; switch to Paper Trading.
  2. Generate API key + secret.
  3. Store in 1Password under entry `xianvec/alpaca-paper`.
  4. Export at runtime:
     ```bash
     export APCA_API_KEY_ID=$(op read 'op://Personal/xianvec-alpaca-paper/api_key_id')
     export APCA_API_SECRET_KEY=$(op read 'op://Personal/xianvec-alpaca-paper/api_secret_key')
     export APCA_API_BASE_URL=https://paper-api.alpaca.markets
     ```
  5. Smoke: place + cancel a test order via `xvn run-setup` against an
     Alpaca-supported asset (BTC/USD on Alpaca crypto, or use SPY equity).
- **Exit:** ≥1 round-trip submit+cancel against paper API succeeds.
- **Unblocks:** Phase 11.1.

### M6. Onboard to Orderly testnet + smoke trade (F5)

- **Trigger:** Phase 11.5 prep.
- **What:**
  1. Complete Orderly's brokered onboarding for an EVM (Mantle) wallet.
     `xvn setup --orderly-onboard` runs the flow per implementation-plan §6.3.
  2. Save `(orderly_key, orderly_secret, orderly_account_id)` in 1Password
     under `xianvec/orderly-testnet`.
  3. Export at runtime:
     ```bash
     export ORDERLY_KEY=$(op read 'op://Personal/xianvec-orderly-testnet/key')
     export ORDERLY_SECRET=$(op read 'op://Personal/xianvec-orderly-testnet/secret')
     export ORDERLY_ACCOUNT_ID=$(op read 'op://Personal/xianvec-orderly-testnet/account_id')
     export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org
     ```
  4. Smoke: place + cancel a tiny `PERP_BTC_USDC` market order against
     testnet via `xvn run-setup --executor orderly --network testnet`.
- **Exit:** ≥1 round-trip submit+cancel against Orderly testnet succeeds; SDK
  errors map to `ExecutorError`.
- **Unblocks:** Phase 11.5.
- **FOLLOWUPS:** F5.

### M7. Mint ERC-8004 agent identity NFTs on Mantle (F4)

- **Trigger:** Phase 11.5 prep, after M6.
- **What:**
  1. Decide whether to use Mantle testnet (Sepolia L2 testnet) or mainnet.
     Mint on testnet first; mainnet only after Phase 9 eval clears.
  2. Fund the deployer wallet with testnet MNT (faucet) or mainnet MNT
     (~$5–20 worth).
  3. Update `identity/vectors_{off,on}.agent.json`:
     - `code_commit`: replace `PENDING` with `git rev-parse HEAD` at the time
       of the run.
     - `contact`: replace `PENDING` with an email or GitHub URL.
     - `vectors_on.manifest_hashes`: replace `["PENDING_PHASE_4_2_EXTRACTION"]`
       with the actual `Manifest::content_hash()` of the production vector
       from M2.
  4. Set `identity.enabled = true` in `config/default.toml` (or per-env override).
  5. Mint:
     ```bash
     export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # testnet
     export MANTLE_DEPLOYER_KEY=$(op read 'op://Personal/xianvec-mantle/deployer_pk')
     cargo run --release -p xianvec-identity --bin mint-identity -- \
       --manifest identity/vectors_off.agent.json
     cargo run --release -p xianvec-identity --bin mint-identity -- \
       --manifest identity/vectors_on.agent.json
     ```
  6. Save the resulting (token_id, contract_addr) pair into the manifest
     and commit.
- **Exit:** both NFTs minted on the chosen network; `identity/<arm>.agent.json`
  has populated identity fields; `xvn` runs without `Mantle creds missing`
  errors when `identity.enabled = true`.
- **Unblocks:** Phase 11.5.
- **FOLLOWUPS:** F4. **xianvec-identity is opt-in** — keep it excluded from
  `default-members` in `Cargo.toml`; explicit `cargo build -p xianvec-identity`
  to compile.

---

## Tier 3 — secrets, models, datasets (one-time setup)

### M8. Anthropic API key (or alternative)

- **What:** sign up at <https://console.anthropic.com>; create a key.
- **Save:** `op://Personal/xianvec-anthropic/api_key`.
- **Export:**
  ```bash
  export ANTHROPIC_API_KEY=$(op read 'op://Personal/xianvec-anthropic/api_key')
  ```
- **Cost rough estimate:** Phase 9 backtest = 100–300 setups × 1 brief ≈
  $1–5 with Haiku; with Opus reasoning, $20–60. Prefer Haiku in CI.

### M9. OpenAI / OpenRouter / Together / Groq key (optional)

- **What:** any OpenAI-compat endpoint works. OpenRouter recommended for
  multi-model evaluation.
- **Save:** `op://Personal/xianvec-openai/api_key`.
- **Export:**
  ```bash
  export OPENAI_API_KEY=$(op read 'op://Personal/xianvec-openai/api_key')
  export OPENAI_BASE_URL=https://openrouter.ai/api/v1   # or stay on api.openai.com/v1
  ```

### M10. Download Qwen3-32B GGUF locally

- **What:** download the Q4_K_M GGUF for the dev loop and Q8_0 for the headline.
- **Suggested:**
  ```bash
  cd models
  huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q4_K_M.gguf \
    --local-dir qwen3-32b-q4-gguf --local-dir-use-symlinks False
  huggingface-cli download Qwen/Qwen3-32B-GGUF Qwen_Qwen3-32B-Q8_0.gguf \
    --local-dir qwen3-32b-q8-gguf --local-dir-use-symlinks False
  ```
- **Disk:** Q4 ≈ 17 GB, Q8 ≈ 32 GB.
- **Verify:** `cargo run --release -p xianvec-trader --bin smoke-trader` loads
  the model and emits a `TraderDecision` JSON.

### M11. Download tokenizer.json

- **What:** the Qwen3-32B `tokenizer.json` (separate from the GGUF).
- **Suggested:**
  ```bash
  huggingface-cli download Qwen/Qwen3-32B tokenizer.json \
    --local-dir models/qwen3-32b-q4-gguf --local-dir-use-symlinks False
  ```

### M12. Source paired setups + bars JSON for the backtest

- **What:** the `xvn ab-compare` runner needs:
  - `data/setups/<n>.json` — `Vec<MarketSnapshot>` covering 2022–2024 paired
    setups on BTC-USD (≥100 setups for the headline N).
  - `data/bars/btc_2022_2024.json` — `Vec<MarketBar>` (OHLCV) covering the
    span and granularity that the setups reference.
- **Sourcing options:**
  - Binance public data → polars Parquet → JSON via the existing
    `xianvec-data` pipeline.
  - Coinbase pro CSV → same.
  - The repo's `data/baselines/` may already have a starter dataset; check
    `data/` before sourcing fresh.
- **Setup-id assignment:** each `MarketSnapshot.setup_id` is a `Uuid::new_v4()`
  generated at dataset-build time and persisted alongside the row so re-runs
  pair correctly (Tier 1 fix #1).

---

## Tier 4 — non-blocking research / upstream

### M13. Open the upstream PR against ranger-finance/orderly-connector-rs (F20)

- **What:** ~30–50 LoC PR adding `[features] default = ["solana", "evm"]`,
  making `solana-sdk`/`solana-client`/`solana_vault_cpi` + `ed25519-dalek 1.x`
  optional behind `feature = "solana"`, switching the `evm` feature to
  `ed25519-dalek 2.x`, dropping the `zeroize = "=1.3.0"` exact pin.
- **Workflow:**
  1. Fork `https://github.com/ranger-finance/orderly-connector-rs`.
  2. Branch + apply the diff per FOLLOWUPS F20 scope.
  3. Run their existing tests under both `--features solana` and
     `--features evm`.
  4. Open PR; cite the workspace-side pin conflict (rustls 0.23 / reqwest 0.13
     wants `zeroize ≥ 1.7`) as motivation.
- **Exit:** PR merged + new release published. Then F19 collapses to a
  5-line workspace change.
- **FOLLOWUPS:** F20 (and its downstream, F19).

### M14. Curate `data/probes/` corpus (F13 / Phase 8.5)

- **What:** ~30–60 hand-picked historical market setups: ambiguous regime
  transitions, low-liquidity sessions, hardest historical decisions, flash-crash
  conditions, regulatory edge cases.
- **Workflow:**
  1. Pull candidate setups from a 4-year BTC history (2021–2024).
  2. Hand-tag each as one of the 5 buckets above.
  3. Save under `data/probes/<bucket>/<uuid>.json` as `MarketSnapshot`.
  4. Wire `ProbeRunner` in `xianvec-eval` per implementation-plan §8.5.
- **Trigger:** Phase 9.2 A/B runner stable + want a regression-detection net
  for vector / prompt / model changes.
- **FOLLOWUPS:** F13.

### M15. Source onchain baselines data (F14 / Phase 7.5)

- **What:** Nansen smart-money copy-trader, funding-rate fader, stablecoin
  exchange-inflow risk-off, liquidation cascade fader. Each consumes
  `OnchainPanel` fields already on `MarketSnapshot`.
- **What's needed:** Nansen API access (paid tier), or DefiLlama-like
  aggregator credentials, or scraped public data.
- **Trigger:** post-headline result if onchain comparison is needed for the
  demo narrative.
- **FOLLOWUPS:** F14.

### M16. Bench rig for `target-cpu=native` measurement (F9)

- **What:** controlled thermal state + ≥10 trials per condition.
- **Why manual:** thermal throttling on Apple Silicon swings results 3.2×
  across 5 runs; need to actually pin CPU governor / let the box cool /
  re-measure.
- **Workflow:**
  1. Cold start; close all non-test apps.
  2. Run `cargo run --release -p xianvec-inference --bin smoke-qwen3` 10×
     with default `RUSTFLAGS`.
  3. Cool box; repeat 10× with `RUSTFLAGS="-C target-cpu=native"`.
  4. Compare median + p95 decode/prefill tok/s.
- **Exit:** if win is ≥1.5× and stable, codify in `.cargo/config.toml` (F10).
- **FOLLOWUPS:** F9, F10.

---

## Quick env-var checklist

```bash
# Stage 1 Intern
export ANTHROPIC_API_KEY=...                  # M8
export OPENAI_API_KEY=...   (optional)        # M9
export OPENAI_BASE_URL=...  (optional)        # M9

# Phase 11.1 Alpaca paper
export APCA_API_KEY_ID=...                    # M5
export APCA_API_SECRET_KEY=...                # M5
export APCA_API_BASE_URL=https://paper-api.alpaca.markets

# Phase 11.5 Orderly
export ORDERLY_KEY=...                        # M6
export ORDERLY_SECRET=...                     # M6
export ORDERLY_ACCOUNT_ID=...                 # M6
export ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org

# Phase 11.5 Mantle (only if identity.enabled = true)
export MANTLE_RPC_URL=https://rpc.sepolia.mantle.xyz   # M7
export MANTLE_DEPLOYER_KEY=...                # M7
```

Recommend stashing all of the above in `.envrc` (direnv) at the workspace
root, gitignored, populated via `op read`. Never commit the raw keys.

---

*Last updated: 2026-05-04. Cross-references: `FOLLOWUPS.md`,
`implementation-plan.md` Phases 9–12.*
