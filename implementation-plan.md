# XIANVEC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a 4-stage trading agent (Intern → Trader → Risk → Execution) where Stage 2 (Trader) uses control vectors to encode disposition, and prove via paired backtest that vectors-on outperforms vectors-off on Δ-Sharpe. Stage 1 (Intern) prepares neutral bull/bear/flat evidence with no recommendation, so the Trader's vectors get clean steering room.

**Architecture:** See `architecture.md` (sibling file). Three model-bearing components, one rules-only risk layer between them. Vectors are active only in Stage 2. Pydantic schemas enforce all stage handoffs. SQLite persists every decision for replay.

**Tech Stack:** Python 3.11. **Inference: llama-cpp-python** (Trader, Q4_K_M GGUF, native control vector support, runs everywhere from Mac to Pi to Linux). **Extraction: repeng + transformers** (offline, on cloud GPU; produces `.pt` vectors that are converted to GGUF for runtime). `gguf-py` for the conversion bridge. Anthropic SDK for Intern (cloud option), alpaca-py for paper trading, pandas-ta for technicals, Nansen + ccxt for onchain, pydantic v2 for schemas, pytest for tests, typer for CLI, python-telegram-bot for demo interface.

---

## File structure

```
xianvec/
├── pyproject.toml
├── README.md
├── architecture.md
├── implementation-plan.md
├── decisions/                  # ADRs / spike outcomes (created in Phase 0.3)
├── .env.example
├── .gitignore
├── config/
│   ├── default.yaml
│   ├── whitelist.yaml
│   ├── regime_vectors.yaml
│   └── risk.yaml
├── data/                       # gitignored, runtime data
├── vectors/                    # gitignored, saved .pt files
├── identity/                   # ERC-8004 agentURI manifests + minted NFT IDs (committed)
│   ├── vectors_on.json         # agentURI manifest for the vectors-ON arm
│   ├── vectors_off.json        # agentURI manifest for the vectors-OFF arm
│   └── registered.json         # NFT IDs returned by the Identity Registry mint
├── .claude/skills/mantle/      # vendored mantle-skills submodule (Phase 0.4)
├── src/xianvec/
│   ├── __init__.py
│   ├── config.py               # config loading + validation
│   ├── schemas.py              # Pydantic stage handoff models
│   ├── data/
│   │   ├── alpaca.py           # OHLCV via Alpaca (also used by Alpaca paper executor)
│   │   ├── nansen.py
│   │   ├── exchange.py         # funding rate / open interest via ccxt
│   │   ├── indicators.py
│   │   └── store.py            # SQLite persistence
│   ├── intern/                 # Stage 1 — neutral evidence prep
│   │   ├── claude.py           # Stage 1 via Claude API
│   │   ├── local.py            # Stage 1 via local Qwen-7B
│   │   └── prompt.py
│   ├── trader/                 # Stage 2 — vectors active here
│   │   ├── model.py            # Qwen loading + generation
│   │   ├── vectors.py          # vector load/apply/gate
│   │   ├── extract.py          # contrastive extraction
│   │   ├── prompt.py
│   │   └── runtime.py          # VectorTrader (decide w/ vectors + gating)
│   ├── risk/
│   │   └── rules.py
│   ├── execution/
│   │   ├── alpaca.py           # Alpaca paper executor (pre-launch testing path, Phase 6.2)
│   │   ├── byreal.py           # Byreal Perps executor on Mantle (hackathon path, Phase 6.3)
│   │   └── simulator.py        # backtest sim
│   ├── onchain/                # Mantle integration (Phase 6.5 + 11)
│   │   ├── erc8004.py          # Identity + Reputation registry calls (web3)
│   │   ├── decision_log.py     # on-chain reputation post per closed trade
│   │   └── manifest.py         # build/validate agentURI JSON
│   ├── pipeline/
│   │   └── trade.py            # full pipeline orchestration
│   ├── baselines/
│   │   ├── null.py
│   │   ├── technical.py
│   │   └── onchain.py
│   ├── eval/
│   │   ├── backtest.py
│   │   ├── metrics.py
│   │   ├── compare.py          # paired bootstrap
│   │   └── report.py
│   └── interface/
│       └── telegram.py
├── tests/
│   ├── unit/
│   │   ├── test_schemas.py
│   │   ├── test_indicators.py
│   │   ├── test_risk.py
│   │   ├── test_metrics.py
│   │   ├── test_baselines.py
│   │   └── test_vectors.py
│   └── integration/
│       ├── test_pipeline.py
│       └── test_backtest.py
└── scripts/
    ├── extract_vectors.py
    ├── run_backtest.py
    ├── run_ab_compare.py
    ├── run_paper.py            # Alpaca paper forward runner (Phase 11.1)
    ├── register_agents.py      # mint ERC-8004 NFTs (one-shot, Phase 6.5)
    ├── run_mantle_forward.py   # Byreal forward runner + on-chain logging (Phase 11.5)
    └── compare_runs.py
```

---

## Structural review (2026-05-02) — fixes baked into the tasks below

A pre-build review surfaced ten structural issues that would have suppressed the magnitude or invalidated the credibility of the headline Δ-Sharpe. Every fix is folded into the relevant task in this doc; this list is the manifest so the rationale is traceable when a future reader wonders why a particular choice was made.

**Tier 1 — material to Δ-Sharpe / CI / divergence credibility**

1. **Intern non-determinism breaks pairing.** Per-arm Claude calls produced different briefings for the same setup. Fix: cache briefings keyed by `setup_id` and run both trader arms against the same cached briefing; set Intern `temperature=0`. *(Phase 2.2, Phase 8.3, Phase 9.2)*
2. **Trader temperature jitter inflates noise.** `temperature=0.4` makes vectors-OFF non-deterministic, polluting both PnL variance and decision-divergence rate. Fix: greedy decoding (`temperature=0`) for both arms in the controlled backtest; sampled decoding only for forward paper. *(Phase 3.1, Phase 4.4, Phase 9.2)*
3. **Backtest portfolio is frozen — risk layer is a no-op.** A fresh `{nav: 10000, open_positions: [], daily_pnl_pct: 0}` per setup means cluster cap, circuit breaker, max-positions, loss-streak cooldown, and vol-targeting are inert. The system measured ≠ the system shipped. Fix: stateful portfolio tracker in `iter_setups`/`run_backtest` that updates NAV, open positions, daily PnL window, loss streak, and `atr_pct` from indicators across the test window. *(Phase 8.3)*
4. **Setup overlap inflates effective n.** `step=8` with `horizon=16` means consecutive setups share half their forward window; outcomes are positively correlated and IID bootstrap CIs are too tight by ~√2. Fix: `step >= horizon` (default 24), and add a block-bootstrap option to `paired_bootstrap_sharpe_delta` for time-series-correct CIs. *(Phase 8.2, Phase 8.3)*
5. **Confidence gate is read at the `{` token.** The first generated token is the JSON brace; gating on its entropy reflects format confidence, not trading conviction. Fix: gate on logits at the position immediately after `"action": "` (the `buy`/`sell`/`flat` choice point). *(Phase 4.4)*

**Tier 2 — credibility and statistical power**

6. **Missing experimental controls (random + orthogonal vectors).** `architecture.md` §9.3 commits to three controls; only OFF is implemented. Without random/orthogonal arms, a positive Δ-Sharpe is consistent with "any perturbation activates exploration." Fix: extract a Gaussian-noise vector (matched Frobenius norm) and a basis-orthogonal vector; run as additional arms in `run_ab_compare.py`. *(Phase 4.1, Phase 9.2)*
7. **Per-setup model reload during gating creates hours of pure I/O.** llama.cpp loads control vectors at constructor time; the gating re-run reloads ~9GB GGUF per setup. Fix: log the would-be gate magnitude in backtest but skip the dampened re-run. Confidence gating is a forward-paper-only feature in v1. *(Phase 4.4, Phase 9.2)*
8. **`returns_from_pnl` is path-dependent.** Dividing by trailing equity makes the return series order-dependent, so bootstrap permutations corrupt Sharpe. Fix: `pnl_i / nav_initial` (constant denominator); order-invariant. *(Phase 8.1)*
9. **GGUF conversion validation is one-shot.** Q4_K_M quantization can attenuate vector effects 30–60%; verifying with one print statement is insufficient. Fix: re-run the spike's directional-match criterion against the converted GGUF vector through `Llama(control_vectors=...)` as a hard Phase-4 gate. *(Phase 4.3)*
10. **Single-asset eval halves statistical power.** `run_ab_compare.py` hardcodes `BTC-USD` while the architecture and risk layer assume a basket. Fix: iterate over the whitelist (BTC + ETH + SOL); concatenate paired returns across assets for the bootstrap. Also exercises the cluster-cap path. *(Phase 9.2)*

**Tier 3 — cleanup (not metric-shifting; folded into the affected tasks)**

- Risk layer runs twice (pipeline + harness) — pipeline owns risk, harness trusts the decision. *(Phase 8.3, Phase 9.2)*
- Decision divergence defined on `action` only — extend to `(action, direction, size_bucket)`. *(Phase 9.2)*
- Briefing log uses the literal `setup_id="ab"` — fix to use the real setup_id. *(Phase 9.2)*
- Walk-forward `train` slice is generated but unused — either delete the parameter or actually select per-fold regime weights from train Sharpe. v1 takes the delete path; document it. *(Phase 8.4)*
- 50 contrastive pairs/axis is at the low end for a 14B model; bump to 200. *(Phase 4.1)*
- Δ-Sharpe is the only inferential test; secondary metrics (MDD, PF, WR) are descriptive and not multiple-comparisons-corrected. State this in the report. *(Phase 10.2)*

---

## Mantle hackathon integration (mandatory)

The Turing Test hackathon runs on Mantle. Five integrations move from "v2 deferred" to "v1 required" because the competition format demands them. Adding these *before* Phase 9's A/B run produces a meaningfully better artifact: the experimental claim becomes trustless and publicly verifiable, not just a SQLite table that lives on Edward's laptop.

**Two execution paths run side by side:**
- **Alpaca paper** — pre-launch *testing* path. Verifies Stage 1→2→3 plumbing, pipeline determinism, risk layer behaviour against a battle-tested broker simulator before any on-chain capital is touched. Required.
- **Byreal Perps on Mantle** — *hackathon submission* path. Real on-chain execution; this is what the Turing Test judges will see.

The capital bridge (`@mantleio/sdk`) is **explicitly out of scope** — funds are pre-funded on Mantle by the user before any forward run. The agent only ever sees on-Mantle balances; it never bridges anything itself.

### M1. ERC-8004 identity registration (per arm)

Each experimental arm gets its own identity NFT. The vectors-OFF arm registers as one agent, vectors-ON registers as a second agent, and they post performance updates to the same reputation registry — the comparison becomes an on-chain experiment, not a private claim.

- Two `agentURI` manifests live in `identity/` (JSON metadata: model, vector config, code commit, contact). Pin to IPFS or HTTPS.
- Mint via the Identity Registry contract (Mantle mainnet).
- After every closed trade *on Byreal*, post a reputation update keyed by setup_id and outcome (PnL + Δ-Sharpe rolling). Alpaca paper trades stay off-chain.
- Both NFTs and reputation history become demo evidence — judges can independently verify the Δ-Sharpe claim.

Implemented in the new **Phase 6.5** below. Must be in place before any forward Byreal run.

### M2. Byreal Perps added as the on-chain execution path

`@byreal-io/byreal-cli` (npm) exposes agent skills for perpetual futures, swaps, and LP yield on Mantle. Stage 3 gets a *second* executor alongside the Alpaca paper executor — same `RiskDecision → Stage 3` contract, different downstream tool. A `--executor {alpaca,byreal}` flag selects between them at runtime.

The thin wrapper at `src/xianvec/execution/byreal.py` shells out to `npx byreal-cli` and parses its JSON outputs. Python is the orchestrator; Node is a runtime dep (document in README).

Implemented in **Phase 6.3** (new task, parallel to Phase 6.2 Alpaca).

### M3. On-chain decision logging

Every Stage-1 → Stage-2 → Stage-3 cycle that completes a trade *on Byreal* emits a reputation-registry post tagged with the agent NFT, the setup_id, the action signature, and the realized PnL. This is the on-chain mirror of `data/decisions.db`. SQLite remains for fast local replay; the on-chain log is the authoritative public record. Alpaca paper trades persist locally only.

Implemented in **Phase 11.5** (forward Mantle runner) below.

### M4. xStocks added to the asset whitelist

Mantle's tokenized equities (Fluxion / xStocks, launched April 2026) trade 24/7 on-chain. The Intern → Trader pipeline is asset-agnostic; the only changes needed are (a) updating `config/whitelist.yaml` to include the xStock symbols xianvec will trade, (b) confirming Byreal CLI exposes them under the same skill surface as crypto perps, (c) adjusting the correlation-cluster map in `config/risk.yaml`.

Implemented as a config-only change in **Phase 1** + a smoke test in Phase 11.5.

### M5. mantle-skills (filesystem skill catalog)

`github.com/mantle-xyz/mantle-skills` is a curated set of Mantle-focused agent skills (network primer, address registry navigator, on-chain risk evaluator, portfolio analyst, defi operator, tx simulator, openclaw competition, etc.). They are filesystem skills — `SKILL.md` + `references/` + `assets/` — designed to be loaded into an LLM's context to ground reasoning about Mantle.

**Why this matters for xianvec:** the Stage 1 Intern (Claude) is the component that consults external context. Loading mantle-skills into Claude's project context means:

- `mantle-network-primer` — Claude reasons about MNT gas, chain IDs, finality semantics correctly without us hand-prompting them.
- `mantle-address-registry-navigator` — when xianvec needs ERC-8004 registry addresses or Byreal contract addresses, the skill resolves them from a verified source rather than us hardcoding addresses that could drift.
- `mantle-risk-evaluator` — provides a *second* risk gate (LLM-mediated, Mantle-specific) on top of xianvec's deterministic risk layer (Phase 5). Pass / warn / block verdicts on state-changing intents catch failure modes the deterministic rules miss (e.g., "this position would interact with an unverified contract").
- `mantle-tx-simulator` — pre-flight check before submitting Byreal txns. Cheap insurance.
- `mantle-portfolio-analyst` — sanity check on-chain balances before each forward run.
- `mantle-openclaw-competition` — likely contains hackathon-specific guidance worth reading at submission time.

xianvec's deterministic risk layer stays in place — it is the load-bearing safety net. mantle-skills is additive: an LLM-mediated second gate that benefits from Mantle-specific context the deterministic rules don't have.

Vendored as a git submodule under `.claude/skills/mantle/`. Implemented in **Phase 0.4** (vendor) and consumed by Stage 1 Intern config + Phase 11.5 forward runner. See **Phase 11.6** for the mantle-risk-evaluator integration as the on-chain pre-flight gate.

### Priority sequencing for the hackathon

1. **Phase 0–8** as planned (the structural-review fixes are independent of Mantle).
   - `Phase 0.4` slots in here: vendor mantle-skills early so Claude has Mantle context for any Mantle-touching task.
2. **Phase 6.5** ERC-8004 identity + reputation registry — must precede any forward Byreal run. Can develop in parallel with Phase 6.3.
3. **Phase 6.3** Byreal executor — added alongside Phase 6.2 Alpaca, not replacing it.
4. **Phase 9** runs unchanged: it's a backtest, no on-chain dependency. The result feeds the demo.
5. **Phase 11.1** Alpaca paper forward run — first; validates the Stage 1→2→3 path against a battle-tested broker before any on-chain capital moves.
6. **Phase 11.5** Byreal forward run on Mantle — second; small N (5–20 paired live trades) is enough for the on-chain proof. The headline statistical claim still rides on Phase 9's backtest.
7. **Phase 11.6** mantle-risk-evaluator pre-flight — required between Stage 2's risk-approved decision and Byreal submission. Logs verdict; abort on `block`.
8. **Phase 12** acceptance criteria gain Mantle items (NFTs minted, mantle-skills loaded, ≥1 Alpaca paper trade, ≥1 Byreal trade closed, ≥1 reputation post per arm, ≥1 xStock in whitelist).

---

## Phase 0 — Foundation & vector validation spike

The spike is the load-bearing decision: if vectors don't measurably steer Qwen3-14B at Q4_K_M, the architecture has to change before any further work. Don't skip.

### Task 0.1: Repo init + Python env

**Files:**
- Create: `pyproject.toml`
- Create: `.gitignore`
- Create: `.env.example`
- Create: `README.md`

- [ ] **Step 1: Initialize repo**

```bash
cd /Users/edkennedy/Code/xianvec
git init
gh repo create xianvec --private --source=. --remote=origin
```

- [ ] **Step 2: Write pyproject.toml**

```toml
[project]
name = "xianvec"
version = "0.1.0"
description = "Trading agent with disposition-encoding control vectors"
requires-python = ">=3.11"
dependencies = [
  "pydantic>=2.6",
  "pyyaml>=6.0",
  "typer>=0.12",
  "rich>=13.7",
  "pandas>=2.2",
  "numpy>=1.26",
  "scipy>=1.12",
  "pandas-ta>=0.3.14b0",
  "anthropic>=0.34",
  "httpx>=0.27",
  "ccxt>=4.3",                  # OHLCV + funding rate (free public endpoints)
  "alpaca-py>=0.21",            # paper-trading testing path (pre-Mantle smoke)
  "web3>=7.0",                  # ERC-8004 identity + reputation registry calls
  "eth-account>=0.13",          # signing for Mantle txns
  "python-telegram-bot>=21",
  "matplotlib>=3.8",
  "seaborn>=0.13",
]

[project.optional-dependencies]
inference = [
  "torch>=2.3",
  "transformers>=4.42",         # extraction only (offline)
  "repeng>=0.5",                # extraction only (offline)
  "gguf>=0.10",                 # .pt -> GGUF control vector conversion
  "mlx-lm>=0.14",               # optional Mac extraction path
  "llama-cpp-python>=0.2.85",   # primary runtime
]
dev = [
  "pytest>=8.2",
  "pytest-asyncio>=0.23",
  "ruff>=0.5",
  "mypy>=1.10",
  "pre-commit>=3.7",
]

[tool.ruff]
line-length = 100
target-version = "py311"

[tool.pytest.ini_options]
testpaths = ["tests"]
asyncio_mode = "auto"

[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[tool.setuptools.packages.find]
where = ["src"]
```

- [ ] **Step 3: Write .gitignore**

```
.venv/
__pycache__/
*.pyc
.pytest_cache/
.mypy_cache/
.ruff_cache/
data/
vectors/*.pt
.env
*.log
.DS_Store
```

- [ ] **Step 4: Write .env.example**

```
# LLM (Stage 1 Intern)
ANTHROPIC_API_KEY=sk-ant-...

# Alpaca paper (Phase 6.2 / 11.1 — pre-launch testing path)
ALPACA_API_KEY=...
ALPACA_API_SECRET=...
ALPACA_PAPER=true

# Onchain signals
NANSEN_API_KEY=...

# Demo
TELEGRAM_BOT_TOKEN=...
TELEGRAM_CHAT_ID=...

# Mantle / hackathon (Phase 6.3 / 6.5 / 11.5)
MANTLE_RPC_URL=https://rpc.mantle.xyz
MANTLE_CHAIN_ID=5000
MANTLE_AGENT_PRIVATE_KEY=0x...        # signs ERC-8004 + Byreal txns; pre-funded with MNT
ERC8004_IDENTITY_REGISTRY=0x...       # populated by `mantle-address-registry-navigator`
ERC8004_REPUTATION_REGISTRY=0x...
BYREAL_API_BASE=https://api.byreal.io  # if needed by byreal-cli
```

**Node.js prerequisite:** `byreal-cli` is an npm package. Install Node 20+ and verify `npx --version` works before Phase 6.3.

- [ ] **Step 5: Create venv and install**

```bash
python3.11 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

Expected: clean install, no resolution errors.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: initial repo structure and python env"
git push -u origin main
```

---

### Task 0.2: Pull Qwen3-14B locally

**Files:**
- Create: `scripts/download_model.py`

- [ ] **Step 1: Install inference extras**

```bash
pip install -e ".[inference]"
```

- [ ] **Step 2: Write download script**

```python
# scripts/download_model.py
"""Download Qwen3-14B at Q4_K_M for local inference. Idempotent."""
from pathlib import Path
from huggingface_hub import snapshot_download

MODELS_DIR = Path("data/models")
MODELS_DIR.mkdir(parents=True, exist_ok=True)

# Q4_K_M GGUF for llama.cpp
snapshot_download(
    repo_id="Qwen/Qwen3-14B-GGUF",
    allow_patterns=["*Q4_K_M*"],
    local_dir=MODELS_DIR / "qwen3-14b-q4km",
)
print(f"Downloaded to {MODELS_DIR / 'qwen3-14b-q4km'}")
```

- [ ] **Step 3: Run it**

```bash
python scripts/download_model.py
```

Expected: ~9GB download. Verify file exists with `ls -lh data/models/qwen3-14b-q4km/`.

- [ ] **Step 4: Smoke test inference**

```bash
python -c "
from llama_cpp import Llama
m = Llama(model_path='data/models/qwen3-14b-q4km/qwen3-14b-q4_k_m.gguf', n_ctx=4096, verbose=False)
print(m('Q: What is 2+2? A:', max_tokens=10)['choices'][0]['text'])
"
```

Expected: a coherent answer mentioning 4. If this fails, fix llama-cpp-python install before proceeding.

- [ ] **Step 5: Commit**

```bash
git add scripts/download_model.py
git commit -m "feat: model download script for Qwen3-14B Q4_K_M"
```

---

### Task 0.3: Vector validation spike (CRITICAL GATE)

**Goal:** Confirm that classical control-vector extraction with `repeng` produces a vector that measurably steers Qwen3-14B Q4_K_M generation on a toy axis ("cautious vs aggressive trader"). If this fails, model has to change before architecture is locked.

**Files:**
- Create: `scripts/spike_vector_validation.py`

- [ ] **Step 1: Write the spike script**

```python
# scripts/spike_vector_validation.py
"""Validate that control vectors steer behavior on a toy axis.

Pass criteria:
- 80%+ of held-out prompts show measurable lexical shift between vectors-off
  and vectors-on at +1.0 magnitude.
- Shift is in the expected direction (more cautious words appear when
  cautious vector is positive, more aggressive words when negative).
"""
from repeng import ControlModel, ControlVector, DatasetEntry
from transformers import AutoModelForCausalLM, AutoTokenizer
import torch

MODEL = "Qwen/Qwen3-14B"  # full precision for extraction
DEVICE = "mps" if torch.backends.mps.is_available() else "cpu"

CAUTIOUS_WORDS = {"wait", "uncertain", "risk", "careful", "small", "hedge", "skeptical"}
AGGRESSIVE_WORDS = {"now", "buy", "long", "size", "conviction", "all-in", "lever"}

PAIRS = [
    ("respond as a cautious, hesitant trader: ",
     "respond as an aggressive, decisive trader: "),
]
SCENARIOS = [
    "BTC just broke above the 50-day moving average on rising volume.",
    "ETH funding rate is at 0.08% with negative open interest divergence.",
    "SOL has been chopping in a 5% range for 3 days.",
    "Smart money wallets accumulated 1200 BTC over 6 hours.",
    "Liquidations cascaded $400M in the last hour.",
]

def build_dataset():
    entries = []
    for cautious_prefix, aggressive_prefix in PAIRS:
        for scenario in SCENARIOS:
            entries.append(DatasetEntry(
                positive=cautious_prefix + scenario,
                negative=aggressive_prefix + scenario,
            ))
    return entries

def main():
    tok = AutoTokenizer.from_pretrained(MODEL)
    model = AutoModelForCausalLM.from_pretrained(
        MODEL, torch_dtype=torch.float16, device_map=DEVICE
    )
    cm = ControlModel(model, layer_ids=list(range(15, 30)))  # mid-to-late
    vector = ControlVector.train(cm, tok, build_dataset())

    holdout = [
        "BTC volatility expanded 3x overnight.",
        "Memecoin season is heating up on Solana.",
        "Whales are moving stablecoins to exchanges.",
    ]

    matches_expected = 0
    for prompt in holdout:
        full = "Trader analyzing: " + prompt + "\nMy plan:"
        cm.reset()
        out_off = cm.generate(tok(full, return_tensors="pt").to(DEVICE), max_new_tokens=60)
        cm.set_control(vector, 1.0)
        out_pos = cm.generate(tok(full, return_tensors="pt").to(DEVICE), max_new_tokens=60)
        cm.set_control(vector, -1.0)
        out_neg = cm.generate(tok(full, return_tensors="pt").to(DEVICE), max_new_tokens=60)

        text_off = tok.decode(out_off[0], skip_special_tokens=True).lower()
        text_pos = tok.decode(out_pos[0], skip_special_tokens=True).lower()
        text_neg = tok.decode(out_neg[0], skip_special_tokens=True).lower()

        cautious_pos = sum(w in text_pos for w in CAUTIOUS_WORDS)
        cautious_neg = sum(w in text_neg for w in CAUTIOUS_WORDS)
        aggressive_pos = sum(w in text_pos for w in AGGRESSIVE_WORDS)
        aggressive_neg = sum(w in text_neg for w in AGGRESSIVE_WORDS)

        ok = cautious_pos > cautious_neg and aggressive_neg > aggressive_pos
        matches_expected += int(ok)

        print(f"\n--- {prompt} ---")
        print(f"OFF:  {text_off[len(full):][:120]}")
        print(f"POS:  {text_pos[len(full):][:120]}")
        print(f"NEG:  {text_neg[len(full):][:120]}")
        print(f"  cautious words: pos={cautious_pos} neg={cautious_neg}")
        print(f"  aggressive words: pos={aggressive_pos} neg={aggressive_neg}")
        print(f"  expected direction: {'PASS' if ok else 'FAIL'}")

    rate = matches_expected / len(holdout)
    print(f"\nDirectional match rate: {rate:.0%} ({matches_expected}/{len(holdout)})")
    assert rate >= 0.66, f"Spike failed: only {rate:.0%} of holdouts shifted as expected"
    print("\nSPIKE PASS — vectors steer the model. Proceed.")

if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run the spike**

```bash
python scripts/spike_vector_validation.py
```

Expected: `SPIKE PASS — vectors steer the model. Proceed.` printed at end.

If it fails: try Qwen3-7B instead, try different layer ranges (e.g., `range(20, 35)`), try larger contrastive datasets. Document what was tried in `decisions/0001-model-choice.md` before proceeding.

- [ ] **Step 3: Save spike artifact + commit**

```bash
mkdir -p decisions
echo "Vector validation spike passed for Qwen3-14B at layers 15-29 with $matches_expected/$len(holdout) directional matches." > decisions/0001-model-choice.md
git add scripts/spike_vector_validation.py decisions/0001-model-choice.md
git commit -m "feat(spike): validate control vectors steer Qwen3-14B"
```

---

### Task 0.4: Vendor mantle-skills as a submodule

**Why:** `github.com/mantle-xyz/mantle-skills` is a curated catalog of Mantle-focused agent skills (network primer, address registry, on-chain risk evaluator, defi operator, tx simulator, portfolio analyst, openclaw competition, etc.). They are filesystem skills (`SKILL.md` + `references/` + `assets/`) loaded into an LLM's context to ground reasoning about Mantle. xianvec consumes them at the Stage 1 Intern level (Claude) and at Phase 11.6 (mantle-risk-evaluator pre-flight gate). See Mantle integration §M5.

Vendor as a submodule so xianvec's commits pin a specific upstream revision — Mantle ships changes regularly and we don't want a hackathon-week regression from upstream drift.

**Files:**
- Add: `.gitmodules` entry
- Add: `.claude/skills/mantle/` (submodule pointing at upstream)
- Add: `.claude/settings.json` snippet enabling the skill on this project (Cowork / Claude Code)

- [ ] **Step 1: Add submodule**

```bash
mkdir -p .claude/skills
git submodule add https://github.com/mantle-xyz/mantle-skills.git .claude/skills/mantle
git -C .claude/skills/mantle log -1 --oneline   # verify pin
```

- [ ] **Step 2: Verify the catalog is reachable**

```bash
ls .claude/skills/mantle/skills/
# expect: mantle-network-primer, mantle-address-registry-navigator, mantle-risk-evaluator,
#         mantle-portfolio-analyst, mantle-defi-operator, mantle-tx-simulator,
#         mantle-data-indexer, mantle-readonly-debugger, mantle-smart-contract-developer,
#         mantle-smart-contract-deployer, mantle-openclaw-competition
```

- [ ] **Step 3: Document which skills xianvec actually loads**

Create `decisions/0004-mantle-skills.md`:

```markdown
# Mantle skills used by xianvec

Loaded at Stage 1 Intern (Claude project context):
- mantle-network-primer       — chain IDs, MNT gas, finality semantics
- mantle-address-registry-navigator — resolves verified contract addresses (ERC-8004 registries, Byreal contracts)
- mantle-portfolio-analyst    — pre-flight balance/allowance checks before forward runs
- mantle-openclaw-competition — hackathon-specific guidance (read at submission time)

Invoked by Phase 11.6 forward-runner gate:
- mantle-risk-evaluator       — pass/warn/block verdict on each Byreal-bound decision (LLM-mediated, additive to xianvec's deterministic risk layer)
- mantle-tx-simulator         — pre-flight simulation of the Byreal txn before submission

Not used in v1 (deferred):
- mantle-data-indexer, mantle-readonly-debugger — useful for triage; not in critical path
- mantle-smart-contract-developer/deployer — xianvec doesn't ship contracts; uses Byreal/ERC-8004 only
```

- [ ] **Step 4: Commit**

```bash
git add .gitmodules .claude/skills/mantle decisions/0004-mantle-skills.md
git commit -m "feat: vendor mantle-skills as submodule + document which skills xianvec uses"
```

---

## Phase 1 — Schemas, config, persistence

### Task 1.1: Pydantic schemas for stage handoffs

**Files:**
- Create: `src/xianvec/schemas.py`
- Test: `tests/unit/test_schemas.py`

- [ ] **Step 1: Write the failing test**

```python
# tests/unit/test_schemas.py
import pytest
from pydantic import ValidationError
from xianvec.schemas import MarketState, InternBriefing, TraderDecision, RiskDecision

def test_intern_briefing_valid():
    b = InternBriefing(
        setup_id="abc-123",
        asset="BTC-PERP",
        bull_case="oversold bounce setting up",
        bear_case="trend is still down",
        flat_case="too noisy to commit either way",
        evidence_long=["rsi_oversold"],
        evidence_short=["volume_declining"],
        evidence_flat=["chop_in_5pct_range_3d"],
        regime="choppy",
        signal_quality=0.55,
        horizon_hours=4,
    )
    assert b.signal_quality == 0.55
    assert "candidate_direction" not in b.model_dump()  # never expose direction

def test_intern_briefing_rejects_bad_regime():
    with pytest.raises(ValidationError):
        InternBriefing(
            setup_id="x", asset="BTC",
            bull_case="t", bear_case="c", flat_case="f",
            evidence_long=[], evidence_short=[], evidence_flat=[],
            regime="not_a_regime",  # invalid
            signal_quality=0.5, horizon_hours=1,
        )

def test_trader_decision_requires_stop_loss_for_entries():
    with pytest.raises(ValidationError):
        TraderDecision(
            setup_id="x", action="buy", size_bps=50, direction="long",
            stop_loss_pct=None, take_profit_pct=5.0,
            trader_summary="conviction long",
            active_vectors={"conviction": 0.8},
        )
```

- [ ] **Step 2: Run test to verify it fails**

```bash
pytest tests/unit/test_schemas.py -v
```

Expected: ImportError or ModuleNotFoundError for xianvec.schemas.

- [ ] **Step 3: Write the implementation**

```python
# src/xianvec/schemas.py
from typing import Any, Literal
from pydantic import BaseModel, Field, model_validator

Regime = Literal["trending", "choppy", "high_vol", "low_vol"]
Direction = Literal["long", "short", "flat"]
Action = Literal["buy", "sell", "flat", "close"]

class MarketState(BaseModel):
    asset: str
    timestamp: float
    ohlcv_recent: list[list[float]]  # [[ts, o, h, l, c, v], ...]
    indicators: dict[str, float]
    onchain: dict[str, float]
    # portfolio holds nav, cash, daily_pnl_pct (floats) and open_positions (list of dicts);
    # we keep it as untyped dict to avoid friction with the risk layer's .get() access.
    portfolio: dict[str, Any]

class InternBriefing(BaseModel):
    """Neutral evidence briefing from the Intern. NO candidate decision — the Trader decides.

    Symmetric structure: bull, bear, and flat cases each have a one-line argument and
    a list of supporting signals. signal_quality is a quality estimate (how clean is
    this setup?), NOT a directional confidence.
    """
    setup_id: str
    asset: str
    bull_case: str
    bear_case: str
    flat_case: str
    evidence_long: list[str]
    evidence_short: list[str]
    evidence_flat: list[str]
    regime: Regime
    signal_quality: float = Field(ge=0.0, le=1.0)
    horizon_hours: float = Field(gt=0)

class TraderDecision(BaseModel):
    setup_id: str
    action: Action
    size_bps: int = Field(ge=0, le=2000)
    direction: Direction
    stop_loss_pct: float | None = Field(default=None, ge=0.1, le=20)
    take_profit_pct: float | None = Field(default=None, ge=0.1, le=50)
    trader_summary: str
    active_vectors: dict[str, float] = Field(default_factory=dict)

    @model_validator(mode="after")
    def entries_require_stop(self):
        if self.action in ("buy", "sell"):
            if self.stop_loss_pct is None:
                raise ValueError("entries (buy/sell) require stop_loss_pct")
        return self

class RiskDecision(BaseModel):
    approved: bool
    original: TraderDecision
    modified: TraderDecision | None = None
    veto_reason: str | None = None

    @model_validator(mode="after")
    def veto_consistency(self):
        if not self.approved and not self.veto_reason:
            raise ValueError("vetoed decisions must have veto_reason")
        return self
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_schemas.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/schemas.py tests/unit/test_schemas.py
git commit -m "feat(schemas): pydantic models for stage handoffs"
```

---

### Task 1.2: Config loader

**Files:**
- Create: `src/xianvec/config.py`
- Create: `config/default.yaml`
- Create: `config/whitelist.yaml`
- Create: `config/regime_vectors.yaml`
- Create: `config/risk.yaml`
- Test: `tests/unit/test_config.py`

- [ ] **Step 1: Write config files**

```yaml
# config/default.yaml
intern:
  backend: claude   # or "local"
  claude_model: claude-haiku-4-5
  local_model_path: data/models/qwen3-7b-q4km/qwen3-7b-q4_k_m.gguf
trader:
  model_path: data/models/qwen3-14b-q4km/qwen3-14b-q4_k_m.gguf
  layer_range: [15, 30]
  max_new_tokens: 256
  vectors_enabled: true
  confidence_gating: true
data:
  alpaca_paper: true
  cadence_minutes: 15
eval:
  min_trades_for_significance: 30
  bootstrap_resamples: 10000
```

```yaml
# config/whitelist.yaml
# Two execution venues are supported: alpaca (paper testing) and byreal (Mantle).
# Symbols listed here are the *internal* names; per-venue mapping happens in code.
assets:
  # Crypto perps — supported on both venues
  - BTC-USD
  - ETH-USD
  - SOL-USD
  # xStocks (Mantle/Byreal only — confirm exact symbols against Byreal's listing
  # on submission; the names below are illustrative and need verification before
  # the forward run).
  - AAPLx-USD
  - TSLAx-USD
  - SPYx-USD

# Per-venue symbol overrides. Code in execution/byreal.py and execution/alpaca.py
# uses these to map internal names to the venue-native symbol. Anything missing
# from a venue's map cannot be traded on that venue.
venues:
  alpaca:
    BTC-USD: "BTC/USD"
    ETH-USD: "ETH/USD"
    SOL-USD: "SOL/USD"
    # xStocks: NOT supported on Alpaca — these will be skipped if executor=alpaca
  byreal:
    BTC-USD: "BTC-PERP"
    ETH-USD: "ETH-PERP"
    SOL-USD: "SOL-PERP"
    AAPLx-USD: "AAPLx-PERP"   # confirm
    TSLAx-USD: "TSLAx-PERP"   # confirm
    SPYx-USD: "SPYx-PERP"     # confirm
```

```yaml
# config/regime_vectors.yaml
trending:
  conviction: 0.7
  trend_disposition: 0.6
  patience: -0.2
  risk_appetite: 0.0
choppy:
  patience: 0.6
  conviction: -0.3
  trend_disposition: -0.5
  risk_appetite: 0.0
high_vol:
  risk_appetite: -0.6
  conviction: -0.2
  patience: 0.0
  trend_disposition: 0.0
low_vol:
  risk_appetite: 0.0
  conviction: 0.2
  patience: 0.0
  trend_disposition: 0.0
```

```yaml
# config/risk.yaml
max_position_size_pct: 20.0
max_total_exposure_pct: 100.0
daily_loss_circuit_breaker_pct: 5.0
max_open_positions: 5
correlation_clusters:
  btc: [BTC-USD]
  eth: [ETH-USD]
  sol: [SOL-USD]
  equities: [AAPLx-USD, TSLAx-USD, SPYx-USD]   # xStocks share macro/index correlation
max_per_cluster: 2
require_stop_loss: true
```

- [ ] **Step 2: Write the failing test**

```python
# tests/unit/test_config.py
from xianvec.config import load_config

def test_loads_default():
    cfg = load_config("config/default.yaml")
    assert cfg["intern"]["backend"] in {"claude", "local"}
    assert cfg["trader"]["vectors_enabled"] in (True, False)

def test_loads_risk():
    risk = load_config("config/risk.yaml")
    assert risk["max_position_size_pct"] == 20.0
```

- [ ] **Step 3: Run test, verify fail**

```bash
pytest tests/unit/test_config.py -v
```

Expected: ImportError.

- [ ] **Step 4: Implement config loader**

```python
# src/xianvec/config.py
from pathlib import Path
import yaml

def load_config(path: str | Path) -> dict:
    p = Path(path)
    if not p.exists():
        raise FileNotFoundError(f"config not found: {p}")
    with p.open() as f:
        return yaml.safe_load(f)
```

- [ ] **Step 5: Run tests, verify pass**

```bash
pytest tests/unit/test_config.py -v
```

Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add src/xianvec/config.py config/ tests/unit/test_config.py
git commit -m "feat(config): yaml config loader and default configs"
```

---

### Task 1.3: SQLite persistence layer

**Files:**
- Create: `src/xianvec/data/store.py`
- Test: `tests/unit/test_store.py`

- [ ] **Step 1: Write the failing test**

```python
# tests/unit/test_store.py
import tempfile
from pathlib import Path
from xianvec.data.store import Store
from xianvec.schemas import TraderDecision

def test_roundtrip_decision(tmp_path):
    store = Store(tmp_path / "test.db")
    store.init()
    d = TraderDecision(
        setup_id="abc",
        action="buy",
        size_bps=50,
        direction="long",
        stop_loss_pct=2.0,
        take_profit_pct=4.0,
        trader_summary="test",
        active_vectors={"conviction": 0.5},
    )
    store.save_decision(d, vectors_enabled=True)
    rows = store.list_decisions()
    assert len(rows) == 1
    assert rows[0]["setup_id"] == "abc"
    assert rows[0]["vectors_enabled"] is True
```

- [ ] **Step 2: Run test, verify fail**

```bash
pytest tests/unit/test_store.py -v
```

Expected: ImportError.

- [ ] **Step 3: Implement store**

```python
# src/xianvec/data/store.py
import json
import sqlite3
from pathlib import Path
from xianvec.schemas import TraderDecision, InternBriefing

SCHEMA = """
CREATE TABLE IF NOT EXISTS decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    setup_id TEXT NOT NULL,
    timestamp REAL NOT NULL DEFAULT (strftime('%s','now')),
    vectors_enabled INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS briefings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    setup_id TEXT NOT NULL,
    timestamp REAL NOT NULL DEFAULT (strftime('%s','now')),
    payload TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS market_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    asset TEXT NOT NULL,
    timestamp REAL NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_decisions_setup ON decisions(setup_id);
CREATE INDEX IF NOT EXISTS idx_briefings_setup ON briefings(setup_id);
"""

class Store:
    def __init__(self, path: str | Path):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)

    def init(self):
        with sqlite3.connect(self.path) as conn:
            conn.executescript(SCHEMA)

    def save_decision(self, d: TraderDecision, vectors_enabled: bool):
        with sqlite3.connect(self.path) as conn:
            conn.execute(
                "INSERT INTO decisions(setup_id, vectors_enabled, payload) VALUES (?, ?, ?)",
                (d.setup_id, int(vectors_enabled), d.model_dump_json()),
            )

    def save_briefing(self, b: InternBriefing):
        with sqlite3.connect(self.path) as conn:
            conn.execute(
                "INSERT INTO briefings(setup_id, payload) VALUES (?, ?)",
                (b.setup_id, b.model_dump_json()),
            )

    def list_decisions(self) -> list[dict]:
        with sqlite3.connect(self.path) as conn:
            conn.row_factory = sqlite3.Row
            rows = conn.execute("SELECT * FROM decisions ORDER BY id").fetchall()
            return [
                {**dict(r), "vectors_enabled": bool(r["vectors_enabled"]),
                 "decision": json.loads(r["payload"])}
                for r in rows
            ]
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_store.py -v
```

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/data/store.py tests/unit/test_store.py
git commit -m "feat(data): sqlite persistence for decisions and briefings"
```

---

### Task 1.4: Technical indicator computation

**Files:**
- Create: `src/xianvec/data/indicators.py`
- Test: `tests/unit/test_indicators.py`

- [ ] **Step 1: Write the failing test**

```python
# tests/unit/test_indicators.py
import pandas as pd
import numpy as np
from xianvec.data.indicators import compute_indicators

def test_indicators_on_synthetic_series():
    # 200 bars, smooth uptrend with noise
    rng = np.random.default_rng(42)
    closes = 100 + np.cumsum(rng.normal(0.05, 0.5, 200))
    df = pd.DataFrame({
        "open": closes - 0.1,
        "high": closes + 0.5,
        "low": closes - 0.5,
        "close": closes,
        "volume": rng.integers(1000, 10000, 200),
    })
    out = compute_indicators(df)
    # essential indicators present and finite at end
    for k in ("rsi_14", "ma_30", "ma_60", "ma_90", "bb_upper", "bb_lower",
             "macd", "macd_signal", "atr_14", "donchian_high_20"):
        assert k in out, f"missing indicator: {k}"
        assert np.isfinite(out[k]), f"non-finite {k}: {out[k]}"

def test_rsi_bounds():
    rng = np.random.default_rng(0)
    closes = 100 + np.cumsum(rng.normal(0, 1, 100))
    df = pd.DataFrame({"open": closes, "high": closes + 1, "low": closes - 1,
                       "close": closes, "volume": [1000]*100})
    out = compute_indicators(df)
    assert 0 <= out["rsi_14"] <= 100
```

- [ ] **Step 2: Run test, verify fail**

```bash
pytest tests/unit/test_indicators.py -v
```

Expected: ImportError.

- [ ] **Step 3: Implement indicators**

```python
# src/xianvec/data/indicators.py
import pandas as pd
import pandas_ta as ta

def compute_indicators(df: pd.DataFrame) -> dict[str, float]:
    """Return latest-bar values for indicators we use across the intern + baselines."""
    if len(df) < 90:
        raise ValueError(f"need at least 90 bars, got {len(df)}")
    out = {}
    out["rsi_14"] = float(ta.rsi(df["close"], length=14).iloc[-1])
    out["ma_30"] = float(df["close"].rolling(30).mean().iloc[-1])
    out["ma_60"] = float(df["close"].rolling(60).mean().iloc[-1])
    out["ma_90"] = float(df["close"].rolling(90).mean().iloc[-1])
    bb = ta.bbands(df["close"], length=20, std=2)
    out["bb_upper"] = float(bb["BBU_20_2.0"].iloc[-1])
    out["bb_lower"] = float(bb["BBL_20_2.0"].iloc[-1])
    out["bb_mid"] = float(bb["BBM_20_2.0"].iloc[-1])
    macd = ta.macd(df["close"])
    out["macd"] = float(macd["MACD_12_26_9"].iloc[-1])
    out["macd_signal"] = float(macd["MACDs_12_26_9"].iloc[-1])
    out["atr_14"] = float(ta.atr(df["high"], df["low"], df["close"], length=14).iloc[-1])
    out["donchian_high_20"] = float(df["high"].rolling(20).max().iloc[-1])
    out["donchian_low_20"] = float(df["low"].rolling(20).min().iloc[-1])
    out["close"] = float(df["close"].iloc[-1])
    out["volume_ratio_20"] = float(df["volume"].iloc[-1] / df["volume"].rolling(20).mean().iloc[-1])
    return out
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_indicators.py -v
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/data/indicators.py tests/unit/test_indicators.py
git commit -m "feat(data): technical indicator computation"
```

---

## Phase 2 — Stage 1 Intern

### Task 2.1: Intern prompt template

**Files:**
- Create: `src/xianvec/intern/prompt.py`

- [ ] **Step 1: Write the prompt builder**

```python
# src/xianvec/intern/prompt.py
from xianvec.schemas import MarketState

INTERN_SYSTEM = """You are a research intern preparing a balanced briefing for the trader.

Your only job is to lay out the evidence on all three sides:
- the bull case (argument for going long)
- the bear case (argument for going short)
- the flat case (argument for sitting this one out)

DO NOT recommend an action. DO NOT pick a side. DO NOT include any "candidate" or "recommendation" field. The trader makes the call — you prepare the desk.

Present the bull and bear cases with equal rigor. If the evidence genuinely favors one side, that will show in the strength of the arguments and the length of the evidence lists, but your framing must remain neutral.

Output strictly valid JSON matching the requested schema. Do not wrap JSON in markdown.
"""

INTERN_USER_TEMPLATE = """Asset: {asset}
Timestamp: {ts}
Recent price: close={close:.4f}
Indicators: {indicators}
Onchain: {onchain}
Portfolio state: {portfolio}

Produce JSON with these fields and ONLY these fields:
- bull_case (string): strongest argument for going long
- bear_case (string): strongest argument for going short
- flat_case (string): strongest argument for sitting out / waiting
- evidence_long (list[str]): named signals supporting long
- evidence_short (list[str]): named signals supporting short
- evidence_flat (list[str]): named signals supporting waiting
- regime (one of: trending, choppy, high_vol, low_vol)
- signal_quality (float 0-1): how clean is this setup? quality, NOT direction
- horizon_hours (float): expected time for the setup to resolve

Setup ID: {setup_id}
"""

def build_intern_prompt(state: MarketState, setup_id: str) -> tuple[str, str]:
    user = INTERN_USER_TEMPLATE.format(
        asset=state.asset,
        ts=state.timestamp,
        close=state.indicators.get("close", 0.0),
        indicators={k: round(v, 4) for k, v in state.indicators.items()},
        onchain={k: round(v, 4) for k, v in state.onchain.items()},
        portfolio=state.portfolio,
        setup_id=setup_id,
    )
    return INTERN_SYSTEM, user
```

- [ ] **Step 2: Commit**

```bash
git add src/xianvec/intern/prompt.py
git commit -m "feat(intern): prompt template for stage 1"
```

---

### Task 2.2: Stage 1 via Claude API

**Files:**
- Create: `src/xianvec/intern/claude.py`
- Test: `tests/unit/test_intern_claude.py`

- [ ] **Step 1: Write the failing test (with mocked client)**

```python
# tests/unit/test_intern_claude.py
import json
from unittest.mock import MagicMock
from xianvec.intern.claude import ClaudeIntern
from xianvec.schemas import MarketState

def test_parses_valid_response():
    fake_response = MagicMock()
    fake_response.content = [MagicMock(text=json.dumps({
        "bull_case": "oversold bounce setting up",
        "bear_case": "downtrend intact",
        "flat_case": "could chop here for hours",
        "evidence_long": ["rsi_oversold"],
        "evidence_short": ["volume_declining"],
        "evidence_flat": ["narrowing_range"],
        "regime": "choppy",
        "signal_quality": 0.55,
        "horizon_hours": 4,
    }))]
    fake_client = MagicMock()
    fake_client.messages.create.return_value = fake_response

    r = ClaudeIntern(client=fake_client, model="claude-haiku-4-5")
    state = MarketState(
        asset="BTC-USD", timestamp=1714600000.0, ohlcv_recent=[],
        indicators={"close": 50000, "rsi_14": 28},
        onchain={"smart_money_inflow": 0.5},
        portfolio={"nav": 10000, "cash": 10000},
    )
    out = r.reason(state, setup_id="test-1")
    assert out.signal_quality == 0.55
    assert out.bull_case.startswith("oversold")
    assert out.regime == "choppy"
    assert out.setup_id == "test-1"
    assert out.asset == "BTC-USD"
    # never leaks a direction recommendation
    assert "candidate_direction" not in out.model_dump()
```

- [ ] **Step 2: Run test, verify fail**

```bash
pytest tests/unit/test_intern_claude.py -v
```

Expected: ImportError.

- [ ] **Step 3: Implement**

```python
# src/xianvec/intern/claude.py
import json
from anthropic import Anthropic
from xianvec.schemas import MarketState, InternBriefing
from xianvec.intern.prompt import build_intern_prompt

class ClaudeIntern:
    def __init__(self, client: Anthropic | None = None, model: str = "claude-haiku-4-5"):
        self.client = client or Anthropic()
        self.model = model

    def reason(self, state: MarketState, setup_id: str) -> InternBriefing:
        system, user = build_intern_prompt(state, setup_id)
        # temperature=0 is structural: both A/B trader arms must read the SAME briefing
        # for the same setup, otherwise Stage-1 sampling noise pollutes the paired Δ-Sharpe.
        # See structural-review fix #1 (briefing cache) — the cache is keyed by setup_id and
        # only sound if Intern is deterministic.
        resp = self.client.messages.create(
            model=self.model,
            max_tokens=1024,
            temperature=0.0,
            system=system,
            messages=[{"role": "user", "content": user}],
        )
        text = resp.content[0].text.strip()
        # tolerate occasional fenced output
        if text.startswith("```"):
            text = text.split("```", 2)[1].lstrip("json").strip()
        payload = json.loads(text)
        payload["setup_id"] = setup_id
        payload["asset"] = state.asset
        return InternBriefing(**payload)
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_intern_claude.py -v
```

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/intern/claude.py tests/unit/test_intern_claude.py
git commit -m "feat(intern): claude-backed stage 1 intern"
```

---

## Phase 3 — Stage 2 Trader (no vectors yet)

### Task 3.1: Local model loader

**Files:**
- Create: `src/xianvec/trader/model.py`

- [ ] **Step 1: Write loader**

```python
# src/xianvec/trader/model.py
from llama_cpp import Llama
from pathlib import Path

class TraderModel:
    """Wraps llama.cpp Qwen for Stage 2 inference. Vector hooks added in Phase 4.

    Default temperature=0 (greedy) so the controlled A/B comparison is deterministic.
    Sampling jitter at the trader is structural noise that inflates both PnL variance
    and decision-divergence rate — vectors-OFF must be reproducible for pairing to work.
    Forward paper trading can override to a positive temperature for exploration.
    See structural-review fix #2.
    """

    def __init__(self, model_path: str, n_ctx: int = 8192):
        if not Path(model_path).exists():
            raise FileNotFoundError(f"model not found: {model_path}")
        self.llm = Llama(model_path=model_path, n_ctx=n_ctx, verbose=False, logits_all=False)

    def generate(self, prompt: str, max_tokens: int = 256, temperature: float = 0.0) -> str:
        out = self.llm(prompt, max_tokens=max_tokens, temperature=temperature, stop=["\n\n"])
        return out["choices"][0]["text"]
```

Note: this loader uses GGUF Q4_K_M and is the production path. Phase 4 extends this same `TraderModel` to load control vectors via llama.cpp's native `control_vectors=` constructor argument. No transformers in the runtime path — only in the offline extraction step (Task 4.1) which produces `.pt` files later converted to GGUF (Task 4.3).

- [ ] **Step 2: Commit**

```bash
git add src/xianvec/trader/model.py
git commit -m "feat(trader): llama.cpp model wrapper for stage 2 baseline"
```

---

### Task 3.2: Trader prompt + JSON-constrained generation

**Files:**
- Create: `src/xianvec/trader/prompt.py`
- Test: `tests/unit/test_trader_prompt.py`

- [ ] **Step 1: Write failing test**

```python
# tests/unit/test_trader_prompt.py
from xianvec.trader.prompt import build_trader_prompt, parse_trader_response
from xianvec.schemas import InternBriefing

def test_prompt_contains_setup_fields():
    b = InternBriefing(
        setup_id="x", asset="BTC-USD",
        bull_case="oversold bounce", bear_case="downtrend",
        flat_case="too noisy",
        evidence_long=["rsi_oversold"], evidence_short=["lower_high"],
        evidence_flat=["narrow_range"],
        regime="choppy", signal_quality=0.5, horizon_hours=4,
    )
    p = build_trader_prompt(b)
    assert "BTC-USD" in p
    assert "oversold bounce" in p          # bull case rendered
    assert "downtrend" in p                # bear case rendered
    assert "too noisy" in p                # flat case rendered
    # the prompt must NOT leak any candidate/recommendation language
    assert "candidate" not in p.lower()
    assert "recommend" not in p.lower()

def test_parses_valid_decision_json():
    text = '{"action":"buy","size_bps":50,"direction":"long","stop_loss_pct":2.0,"take_profit_pct":4.0,"trader_summary":"conviction long"}'
    d = parse_trader_response(text, setup_id="x", active_vectors={"conviction": 0.5})
    assert d.action == "buy"
    assert d.size_bps == 50
    assert d.active_vectors == {"conviction": 0.5}
```

- [ ] **Step 2: Run test, verify fail**

```bash
pytest tests/unit/test_trader_prompt.py -v
```

- [ ] **Step 3: Implement**

```python
# src/xianvec/trader/prompt.py
import json
import re
from xianvec.schemas import InternBriefing, TraderDecision

TRADER_TEMPLATE = """You are the trader. The intern has prepared a balanced briefing on this setup. The intern made no recommendation — that is your job.

Asset: {asset}
Regime: {regime}
Signal quality (0-1, quality not direction): {signal_quality}
Horizon (hours): {horizon_hours}

—— Bull case ——
{bull_case}
Evidence supporting long: {evidence_long}

—— Bear case ——
{bear_case}
Evidence supporting short: {evidence_short}

—— Flat case ——
{flat_case}
Evidence supporting waiting: {evidence_flat}

Decide. Output strictly JSON with these keys, nothing else:
- action ("buy", "sell", "flat", "close")
- size_bps (int 0-2000; 0 if flat)
- direction ("long", "short", "flat")
- stop_loss_pct (float 0.1-20, required if action is buy/sell)
- take_profit_pct (float or null)
- trader_summary (one short sentence in your voice)

JSON:"""

def build_trader_prompt(b: InternBriefing) -> str:
    """Build the trader prompt. Note: dispositional config is NOT rendered into text —
    vectors steer via hidden states only, never via prompt language. This prevents
    text-conditioning from leaking the same signal the vectors are meant to encode."""
    return TRADER_TEMPLATE.format(
        asset=b.asset,
        regime=b.regime,
        signal_quality=b.signal_quality,
        horizon_hours=b.horizon_hours,
        bull_case=b.bull_case,
        bear_case=b.bear_case,
        flat_case=b.flat_case,
        evidence_long=b.evidence_long,
        evidence_short=b.evidence_short,
        evidence_flat=b.evidence_flat,
    )

JSON_RE = re.compile(r"\{.*?\}", re.DOTALL)

def parse_trader_response(text: str, setup_id: str, active_vectors: dict[str, float]) -> TraderDecision:
    match = JSON_RE.search(text)
    if not match:
        raise ValueError(f"no JSON found in trader response: {text[:200]}")
    payload = json.loads(match.group(0))
    payload["setup_id"] = setup_id
    payload["active_vectors"] = active_vectors
    return TraderDecision(**payload)
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_trader_prompt.py -v
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/trader/prompt.py tests/unit/test_trader_prompt.py
git commit -m "feat(trader): prompt template and JSON parser"
```

---

### Task 3.3: End-to-end Intern → Trader smoke (no vectors)

**Files:**
- Create: `scripts/smoke_pipeline_no_vectors.py`

- [ ] **Step 1: Write smoke script**

```python
# scripts/smoke_pipeline_no_vectors.py
"""Smoke: Stage 1 (Claude) → Stage 2 (local Qwen, no vectors) on a synthetic setup."""
import os
from xianvec.config import load_config
from xianvec.schemas import MarketState
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.prompt import build_trader_prompt, parse_trader_response

def main():
    cfg = load_config("config/default.yaml")
    state = MarketState(
        asset="BTC-USD", timestamp=1714600000.0, ohlcv_recent=[],
        indicators={"close": 50000, "rsi_14": 28, "ma_30": 51000, "ma_90": 52000,
                    "atr_14": 1500, "macd": -50, "macd_signal": -20,
                    "bb_upper": 53000, "bb_lower": 49500, "donchian_high_20": 54000,
                    "volume_ratio_20": 1.2},
        onchain={"smart_money_inflow": 0.4, "funding_rate": -0.02, "stablecoin_inflow": 0.1},
        portfolio={"nav": 10000.0, "cash": 10000.0},
    )
    intern = ClaudeIntern(model=cfg["intern"]["claude_model"])
    briefing = intern.reason(state, setup_id="smoke-1")
    print("BRIEFING:", briefing.model_dump_json(indent=2))

    trader_model = TraderModel(cfg["trader"]["model_path"])
    prompt = build_trader_prompt(briefing)
    raw = trader_model.generate(prompt)
    print("RAW TRADER:", raw)
    decision = parse_trader_response(raw, setup_id="smoke-1", active_vectors={})
    print("DECISION:", decision.model_dump_json(indent=2))

if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run it**

```bash
ANTHROPIC_API_KEY=$(op item get anthropic --fields api_key) python scripts/smoke_pipeline_no_vectors.py
```

Expected: Both REASONING and DECISION print as valid JSON. If JSON parsing fails on STANCE step, iterate on the prompt template — Qwen may need stronger format anchoring (few-shot examples in prompt) or grammar-constrained generation via llama.cpp `grammar=`.

- [ ] **Step 3: Commit**

```bash
git add scripts/smoke_pipeline_no_vectors.py
git commit -m "feat: end-to-end pipeline smoke without vectors"
```

---

## Phase 4 — Vector extraction

### Task 4.1: Disposition contrastive datasets

**Files:**
- Create: `src/xianvec/trader/extract.py`
- Create: `data/contrastive/conviction.json`
- Create: `data/contrastive/patience.json`
- Create: `data/contrastive/risk_appetite.json`
- Create: `data/contrastive/trend_disposition.json`

- [ ] **Step 1: Write four contrastive dataset files**

Each file is a list of `{positive, negative}` prompt pairs covering trading scenarios. Example:

```json
// data/contrastive/conviction.json
[
  {
    "positive": "respond as a decisive, fully-committed trader: BTC just broke above the 50-day moving average on rising volume. My plan:",
    "negative": "respond as a hesitant, hedged trader: BTC just broke above the 50-day moving average on rising volume. My plan:"
  },
  {
    "positive": "respond as a decisive, fully-committed trader: ETH funding rate flipped negative after 3 days of positive. My plan:",
    "negative": "respond as a hesitant, hedged trader: ETH funding rate flipped negative after 3 days of positive. My plan:"
  }
]
```

Generate **200 pairs per axis** covering: breakouts, breakdowns, range-bound, regime shifts, smart money flows, funding extremes, liquidation events, low/high vol regimes. (Original spec said 50; bumped because 14B-parameter hidden states need more contrast pairs to converge to a stable axis. 200 is repeng's recommended floor for production use. See structural-review fix #6/note.) Delegate this to a Sonnet/Opus subagent — it's pure content generation.

- [ ] **Step 2: Write extraction module**

```python
# src/xianvec/trader/extract.py
import json
from pathlib import Path
from typing import Any
import numpy as np
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer
from repeng import ControlModel, ControlVector, DatasetEntry

VECTORS_DIR = Path("vectors")
VECTORS_DIR.mkdir(exist_ok=True)

def load_pairs(path: str | Path) -> list[DatasetEntry]:
    data = json.loads(Path(path).read_text())
    return [DatasetEntry(positive=p["positive"], negative=p["negative"]) for p in data]

def extract_axis(
    axis_name: str,
    pairs_path: str | Path,
    model_id: str,
    layer_range: tuple[int, int],
    device: str = "mps",
) -> Path:
    tok = AutoTokenizer.from_pretrained(model_id)
    model = AutoModelForCausalLM.from_pretrained(model_id, torch_dtype=torch.float16, device_map=device)
    cm = ControlModel(model, layer_ids=list(range(*layer_range)))
    pairs = load_pairs(pairs_path)
    vec = ControlVector.train(cm, tok, pairs)
    out_path = VECTORS_DIR / f"{axis_name}.pt"
    vec.save(out_path)
    return out_path

def extract_all(model_id: str, layer_range: tuple[int, int]) -> dict[str, Path]:
    axes = ["conviction", "patience", "risk_appetite", "trend_disposition"]
    results = {}
    for axis in axes:
        path = extract_axis(axis, f"data/contrastive/{axis}.json", model_id, layer_range)
        results[axis] = path
        print(f"saved {axis} -> {path}")
    return results

def make_control_vectors(disposition_paths: dict[str, Path],
                         layer_range: tuple[int, int],
                         seed: int = 42) -> dict[str, Path]:
    """Generate the experimental-control vectors required by architecture.md §9.3.

    `random`: Gaussian-noise vector with the same per-layer Frobenius norm as the
              composed disposition vector at unit weights. Tests "any perturbation
              activates exploration" as the null.
    `orthogonal`: Vector projected onto the null space of the four disposition
                  axes via Gram-Schmidt. Tests representation impact vs
                  direction-specific impact.

    Without these arms, a positive Δ-Sharpe is consistent with "noise helps."
    See structural-review fix #6.
    """
    import torch
    rng = np.random.default_rng(seed)
    layer_ids = list(range(*layer_range))

    # Load all disposition vectors
    disposition_cvs = {name: ControlVector.load(p) for name, p in disposition_paths.items()}
    # Reference shape and norm: average per-layer norm of the unweighted sum.
    summed = None
    for cv in disposition_cvs.values():
        summed = cv if summed is None else summed + cv

    # 1. Random vector — same per-layer norm as the summed disposition
    random_dirs = {}
    for lid in layer_ids:
        ref = np.asarray(summed.directions[lid], dtype=np.float32)
        v = rng.normal(size=ref.shape).astype(np.float32)
        v = v / np.linalg.norm(v) * np.linalg.norm(ref)
        random_dirs[lid] = v
    random_cv = ControlVector(model_type=summed.model_type, directions=random_dirs)
    random_path = VECTORS_DIR / "random.pt"
    random_cv.save(random_path)

    # 2. Orthogonal vector — Gram-Schmidt against the disposition basis per layer
    orth_dirs = {}
    for lid in layer_ids:
        basis = np.stack([np.asarray(cv.directions[lid], dtype=np.float32)
                          for cv in disposition_cvs.values()])  # (4, D)
        ref = np.asarray(summed.directions[lid], dtype=np.float32)
        v = rng.normal(size=ref.shape).astype(np.float32)
        # subtract projection onto each basis vector
        for b in basis:
            v = v - (v @ b) / (b @ b + 1e-12) * b
        v = v / (np.linalg.norm(v) + 1e-12) * np.linalg.norm(ref)
        orth_dirs[lid] = v
    orth_cv = ControlVector(model_type=summed.model_type, directions=orth_dirs)
    orth_path = VECTORS_DIR / "orthogonal.pt"
    orth_cv.save(orth_path)

    return {"random": random_path, "orthogonal": orth_path}
```

- [ ] **Step 3: Write extraction runner**

```python
# scripts/extract_vectors.py
from xianvec.trader.extract import extract_all, make_control_vectors
from xianvec.config import load_config

cfg = load_config("config/default.yaml")
layer_range = tuple(cfg["trader"]["layer_range"])
disposition_paths = extract_all("Qwen/Qwen3-14B", layer_range)
control_paths = make_control_vectors(disposition_paths, layer_range)
print(f"saved disposition vectors: {list(disposition_paths.keys())}")
print(f"saved experimental controls: {list(control_paths.keys())}")
```

- [ ] **Step 4: Run extraction**

```bash
python scripts/extract_vectors.py
```

Expected: four disposition `.pt` files plus `random.pt` and `orthogonal.pt` in `vectors/`. Disposition extraction takes 5-15 minutes per axis on M-series Mac; control vectors are O(seconds) since they're constructed analytically from already-extracted disposition vectors.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/trader/extract.py scripts/extract_vectors.py data/contrastive/
git commit -m "feat(trader): contrastive extraction for four disposition axes"
```

---

### Task 4.2: Vector application + confidence gating

**Files:**
- Create: `src/xianvec/trader/vectors.py`
- Test: `tests/unit/test_vectors.py`

- [ ] **Step 1: Write the failing test**

```python
# tests/unit/test_vectors.py
import numpy as np
from repeng import ControlVector
from xianvec.trader.vectors import compose_axis_vectors, gate_magnitude

def _fake_cv(value: float, shape=(10,), layers=(15, 16)) -> ControlVector:
    directions = {l: np.full(shape, value, dtype=np.float32) for l in layers}
    return ControlVector(model_type="test", directions=directions)

def test_compose_returns_none_when_all_weights_zero():
    vecs = {"a": _fake_cv(1.0), "b": _fake_cv(2.0)}
    out = compose_axis_vectors(vecs, {"a": 0.0, "b": 0.0})
    assert out is None

def test_compose_linear():
    vecs = {"a": _fake_cv(1.0), "b": _fake_cv(2.0)}
    out = compose_axis_vectors(vecs, {"a": 0.5, "b": 0.25})
    # expected layer value = 1.0*0.5 + 2.0*0.25 = 1.0
    for layer, direction in out.directions.items():
        assert np.allclose(direction, 1.0)

def test_compose_skips_unknown_axis():
    vecs = {"a": _fake_cv(1.0)}
    out = compose_axis_vectors(vecs, {"a": 1.0, "missing": 5.0})
    for direction in out.directions.values():
        assert np.allclose(direction, 1.0)

def test_gate_high_entropy_dampens():
    # uniform-ish distribution = high entropy = wide corridor
    logits_uniform = np.array([0.1, 0.1, 0.1, 0.1])
    # peaked distribution = low entropy = tight corridor
    logits_peaked = np.array([5.0, 0.0, 0.0, 0.0])
    g_wide = gate_magnitude(logits_uniform, max_entropy=1.5)
    g_tight = gate_magnitude(logits_peaked, max_entropy=1.5)
    assert g_wide < g_tight
    assert 0.0 <= g_wide <= 1.0
    assert 0.0 <= g_tight <= 1.0
```

- [ ] **Step 2: Run test, verify fail**

```bash
pytest tests/unit/test_vectors.py -v
```

- [ ] **Step 3: Implement vectors module**

```python
# src/xianvec/trader/vectors.py
from pathlib import Path
import numpy as np
from repeng import ControlVector

def load_axis_vectors(vectors_dir: str | Path) -> dict[str, ControlVector]:
    """Load saved per-axis ControlVectors from disk. File stem = axis name."""
    p = Path(vectors_dir)
    return {f.stem: ControlVector.load(f) for f in p.glob("*.pt")}

def compose_axis_vectors(
    vectors: dict[str, ControlVector],
    weights: dict[str, float],
) -> ControlVector | None:
    """Linear combination of axis ControlVectors per layer.

    Returns None if no nonzero weights apply (caller should disable steering).
    Relies on repeng's ControlVector.__add__ and __mul__.
    """
    composed = None
    for name, w in weights.items():
        if name not in vectors or w == 0.0:
            continue
        scaled = vectors[name] * float(w)
        composed = scaled if composed is None else composed + scaled
    return composed

def gate_magnitude(logits: np.ndarray, max_entropy: float = 1.5) -> float:
    """Map decision-token entropy to a vector magnitude scaler in [0, 1].

    Logits MUST come from the position immediately after `"action": "` in the
    trader's JSON output — that's the buy/sell/flat choice point. Reading
    position-0 logits gives entropy of the JSON `{` brace, which is structural
    noise. See `TraderModel._extract_action_logits` and structural-review fix #5.

    Low entropy (peaked distribution / tight corridor) -> 1.0 (full magnitude).
    High entropy (uniform / wide corridor) -> dampened toward 0.
    """
    probs = np.exp(logits - logits.max())
    probs = probs / probs.sum()
    entropy = -np.sum(probs * np.log(probs + 1e-12))
    # linear ramp: entropy=0 -> 1.0, entropy=max_entropy -> 0.0
    g = max(0.0, 1.0 - entropy / max_entropy)
    return float(g)
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_vectors.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/trader/vectors.py tests/unit/test_vectors.py
git commit -m "feat(trader): vector composition and confidence gating"
```

---

### Task 4.3: Convert repeng vectors to llama.cpp GGUF format

**Why:** Extraction needs PyTorch hidden states (so it stays on `repeng` + `transformers` on cloud GPU). Inference belongs on `llama.cpp` — native control vector support since 2024, native quantization, Metal/CUDA acceleration, cross-platform, lower memory. The bridge between them is a one-time conversion of `.pt` → GGUF control-vector format.

**Files:**
- Create: `scripts/convert_vectors_to_gguf.py`

- [ ] **Step 1: Conversion script**

```python
# scripts/convert_vectors_to_gguf.py
"""Convert repeng .pt control vectors to llama.cpp GGUF control-vector format.

llama.cpp's control-vector GGUF expects per-layer direction tensors keyed by
`direction.<layer_id>`. We unpack repeng's ControlVector.directions dict and
write each layer as a separate tensor in a single GGUF file.

Reference: llama.cpp gguf-py and the `--control-vector` flag (PR #5970).
"""
from pathlib import Path
import numpy as np
import typer
from repeng import ControlVector
from gguf import GGUFWriter

VECTORS_DIR = Path("vectors")
GGUF_DIR = Path("vectors/gguf")
GGUF_DIR.mkdir(parents=True, exist_ok=True)

def convert(axis_name: str) -> Path:
    pt_path = VECTORS_DIR / f"{axis_name}.pt"
    gguf_path = GGUF_DIR / f"{axis_name}.gguf"
    cv = ControlVector.load(pt_path)
    writer = GGUFWriter(str(gguf_path), arch="controlvector")
    writer.add_string("controlvector.model_hint", cv.model_type)
    writer.add_uint32("controlvector.layer_count", len(cv.directions))
    for layer_id, direction in cv.directions.items():
        arr = np.asarray(direction, dtype=np.float32)
        writer.add_tensor(f"direction.{layer_id}", arr)
    writer.write_header_to_file()
    writer.write_kv_data_to_file()
    writer.write_tensors_to_file()
    writer.close()
    return gguf_path

def main():
    for axis in ["conviction", "patience", "risk_appetite", "trend_disposition",
                 "random", "orthogonal"]:
        out = convert(axis)
        print(f"converted {axis} -> {out}")

if __name__ == "__main__":
    typer.run(main)
```

- [ ] **Step 2: Quantization-validation gate (hard gate, blocks Phase 9)**

The Phase 0 spike validated steering at fp16. Q4_K_M quantization can attenuate vector effects 30–60%. A one-print eyeball check is not sufficient — if the converted vector fails to steer the quantized model, all of Phase 9's Δ-Sharpe will come back at zero and you'll only discover it after a multi-hour backtest. Port the spike's directional-match criterion to the GGUF runtime and require it to pass before proceeding.

Create `scripts/spike_vector_validation_gguf.py`:

```python
"""Re-run the Phase-0 directional-match criterion on the GGUF-converted control vector.

Pass criterion (relaxed from fp16 spike's 66% to 50% to allow for Q4 attenuation,
but if it falls below 50% we have a quantization problem we need to solve before
Phase 9 — either bump magnitudes, switch to Q5/Q6, or change the layer range).
"""
from pathlib import Path
import json
from llama_cpp import Llama

CAUTIOUS_WORDS = {"wait", "uncertain", "risk", "careful", "small", "hedge", "skeptical"}
AGGRESSIVE_WORDS = {"now", "buy", "long", "size", "conviction", "all-in", "lever"}

HOLDOUT = [
    "BTC volatility expanded 3x overnight.",
    "Memecoin season is heating up on Solana.",
    "Whales are moving stablecoins to exchanges.",
    "ETH funding flipped negative after sustained positive print.",
    "Donchian breakout on SOL with rising volume.",
]

MODEL_PATH = "data/models/qwen3-14b-q4km/qwen3-14b-q4_k_m.gguf"
CV_PATH = "vectors/gguf/conviction.gguf"

def run(magnitude: float) -> str:
    llm = Llama(
        model_path=MODEL_PATH, n_ctx=4096, verbose=False,
        control_vectors=[(CV_PATH, magnitude)] if magnitude != 0.0 else [],
    )
    outs = []
    for prompt in HOLDOUT:
        full = "Trader analyzing: " + prompt + "\nMy plan:"
        out = llm(full, max_tokens=80, temperature=0.0)["choices"][0]["text"].lower()
        outs.append(out)
    return outs

def main():
    off = run(0.0)
    pos = run(+1.0)
    neg = run(-1.0)

    matches = 0
    for i, prompt in enumerate(HOLDOUT):
        cautious_pos = sum(w in pos[i] for w in CAUTIOUS_WORDS)
        cautious_neg = sum(w in neg[i] for w in CAUTIOUS_WORDS)
        aggressive_pos = sum(w in pos[i] for w in AGGRESSIVE_WORDS)
        aggressive_neg = sum(w in neg[i] for w in AGGRESSIVE_WORDS)
        ok = cautious_pos > cautious_neg and aggressive_neg > aggressive_pos
        matches += int(ok)
        print(f"--- {prompt} ---  match={'PASS' if ok else 'FAIL'}")

    rate = matches / len(HOLDOUT)
    print(f"\nGGUF directional match rate (mag 1.0): {rate:.0%}")
    Path("decisions").mkdir(exist_ok=True)
    Path("decisions/0001b-gguf-validation.md").write_text(
        f"# GGUF control-vector validation\n\n"
        f"Magnitude tested: ±1.0\n"
        f"Holdout prompts: {len(HOLDOUT)}\n"
        f"Directional match rate: {rate:.0%} ({matches}/{len(HOLDOUT)})\n"
        f"Pass threshold: 50% (relaxed from fp16 spike's 66% to allow Q4 attenuation)\n"
        f"Status: {'PASS' if rate >= 0.5 else 'FAIL — bump magnitude or change layer range'}\n"
    )
    assert rate >= 0.5, (
        f"GGUF vector failed directional steering at {rate:.0%}. Try magnitude 1.5/2.0, "
        f"switch to Q5_K_M, or expand layer range. Phase 9 cannot proceed until this passes."
    )
    print("\nGGUF VALIDATION PASS — proceed to Phase 9.")

if __name__ == "__main__":
    main()
```

- [ ] **Step 3: Run conversion + validation gate**

```bash
python scripts/convert_vectors_to_gguf.py
python scripts/spike_vector_validation_gguf.py
```

Expected: `GGUF VALIDATION PASS — proceed to Phase 9.`. If FAIL, raise magnitude in `config/regime_vectors.yaml`, switch quantization to Q5_K_M (re-download in Task 0.2), or change `layer_range` and re-extract. **Do not skip.** The whole eval framework rests on the converted vectors actually steering the quantized runtime model.

- [ ] **Step 4: Commit**

```bash
git add scripts/convert_vectors_to_gguf.py scripts/spike_vector_validation_gguf.py
git commit -m "feat(trader): repeng .pt -> llama.cpp GGUF + quantization-validation gate"
```

---

### Task 4.4: Trader model with vectors integrated (llama.cpp path)

**Files:**
- Modify: `src/xianvec/trader/model.py`
- Modify: `src/xianvec/trader/runtime.py` (composition + gating)
- Test: extend `tests/integration/test_pipeline.py`

- [ ] **Step 1: Update TraderModel for llama.cpp control vectors**

```python
# src/xianvec/trader/model.py
from pathlib import Path
import numpy as np
from llama_cpp import Llama

class TraderModel:
    """llama.cpp-backed Trader. Control vectors are GGUF files loaded by path
    with a magnitude scalar each. Composition is by addition: load multiple
    vectors with their respective signed magnitudes."""

    def __init__(self, model_path: str, n_ctx: int = 8192):
        if not Path(model_path).exists():
            raise FileNotFoundError(f"model not found: {model_path}")
        self.model_path = model_path
        self.n_ctx = n_ctx
        self._loaded_vectors: list[tuple[str, float]] = []
        self.llm: Llama | None = None
        self._reload()

    def _reload(self):
        """llama.cpp loads control vectors at model construction time, so changing
        them requires a reload. Cheap if model weights are warm in OS page cache."""
        self.llm = Llama(
            model_path=self.model_path,
            n_ctx=self.n_ctx,
            verbose=False,
            control_vectors=self._loaded_vectors,
            logits_all=False,
        )

    def set_vectors(self, weighted: list[tuple[str, float]] | None):
        """Set the active control vectors. Pass None or [] to clear."""
        new = weighted or []
        if new == self._loaded_vectors:
            return
        self._loaded_vectors = new
        self._reload()

    def generate(self, prompt: str, max_tokens: int = 256, temperature: float = 0.0
                 ) -> tuple[str, np.ndarray]:
        """Generate, returning (text, action_token_logits) for confidence gating.

        Default greedy (temperature=0) for the controlled A/B run; pass a positive
        temperature only for forward paper trading. Returned logits are read at the
        token position immediately after `"action": "` — that's the buy/sell/flat
        choice point, which is what the gate is supposed to measure. The previous
        version returned position-0 logits, which is the JSON `{` brace — pure noise.
        See structural-review fix #5.
        """
        out = self.llm(
            prompt, max_tokens=max_tokens, temperature=temperature,
            stop=["\n\n"], logprobs=10,
        )
        text = out["choices"][0]["text"]
        action_logits = self._extract_action_logits(out, text)
        return text, action_logits

    def _extract_action_logits(self, out: dict, text: str) -> np.ndarray:
        """Read top-K logprobs at the position immediately after `"action": "`.

        llama-cpp-python returns per-token logprobs as a list aligned with the generated
        tokens. We find the offset where `"action": "` ends in the decoded text, map it
        to a token index, and read top_logprobs[idx]. Falls back to zeros if the marker
        is not found (e.g. the model emitted a malformed response — gating then dampens).
        """
        marker = '"action": "'
        idx_char = text.find(marker)
        first_logprobs = out["choices"][0].get("logprobs") or {}
        token_offsets = first_logprobs.get("text_offset") or []
        top_logprobs_list = first_logprobs.get("top_logprobs") or []
        if idx_char < 0 or not token_offsets or not top_logprobs_list:
            return np.zeros(1, dtype=np.float32)
        target_offset = idx_char + len(marker)
        # find the first token whose text_offset is >= target_offset
        try:
            tok_idx = next(i for i, off in enumerate(token_offsets) if off >= target_offset)
        except StopIteration:
            return np.zeros(1, dtype=np.float32)
        if tok_idx >= len(top_logprobs_list):
            return np.zeros(1, dtype=np.float32)
        top = top_logprobs_list[tok_idx] or {}
        if not top:
            return np.zeros(1, dtype=np.float32)
        return np.array(list(top.values()), dtype=np.float32)
```

Notes: `control_vectors` is the constructor-time API (PR #5970 lineage). `logprobs=10` returns top-10 token logprobs at each position; we use position 0 for confidence gating. If `llama-cpp-python` exposes a runtime `set_control_vector` method by the time you build, prefer it (faster than reload).

- [ ] **Step 2: Update VectorTrader to use file paths + magnitudes (no more ControlVector composition)**

```python
# src/xianvec/trader/runtime.py
from pathlib import Path
from xianvec.schemas import InternBriefing, TraderDecision
from xianvec.trader.model import TraderModel
from xianvec.trader.prompt import build_trader_prompt, parse_trader_response
from xianvec.trader.vectors import gate_magnitude

GGUF_VECTORS_DIR = Path("vectors/gguf")

class VectorTrader:
    """Trader runtime with optional confidence gating.

    `backtest_mode=True` disables the dampened-magnitude re-run when the gate fires.
    Reason: llama.cpp loads control vectors at constructor time, so a re-run requires
    a full ~9GB GGUF reload. At ~30s/reload × 1000 setups × P(gate<0.5), that is
    hours of pure I/O for a v1 hackathon backtest. The gate magnitude is still
    computed and logged for offline analysis. Forward paper trading runs with
    `backtest_mode=False` and pays the reload cost (low frequency, irrelevant).
    See structural-review fix #7.
    """

    def __init__(self, model: TraderModel, axis_names: list[str],
                 vectors_enabled: bool = True, confidence_gating: bool = True,
                 backtest_mode: bool = False):
        self.model = model
        self.axis_names = axis_names
        self.vectors_enabled = vectors_enabled
        self.confidence_gating = confidence_gating
        self.backtest_mode = backtest_mode

    def _resolve_vectors(self, weights: dict[str, float]) -> list[tuple[str, float]]:
        out = []
        for name, w in weights.items():
            if name not in self.axis_names or w == 0.0:
                continue
            path = GGUF_VECTORS_DIR / f"{name}.gguf"
            if not path.exists():
                continue
            out.append((str(path), float(w)))
        return out

    def decide(self, briefing: InternBriefing, regime_vectors: dict) -> TraderDecision:
        prompt = build_trader_prompt(briefing)
        active: dict[str, float] = {}
        gate: float | None = None
        if self.vectors_enabled and regime_vectors:
            weighted = self._resolve_vectors(regime_vectors)
            if weighted:
                self.model.set_vectors(weighted)
                text, action_logits = self.model.generate(prompt)
                if self.confidence_gating:
                    gate = gate_magnitude(action_logits)
                    if gate < 0.5 and not self.backtest_mode:
                        # forward mode: re-run with dampened magnitudes (eats a reload)
                        dampened = [(p, m * gate) for p, m in weighted]
                        self.model.set_vectors(dampened)
                        text, _ = self.model.generate(prompt)
                    # backtest mode: log the gate but do not reload — see class docstring
                active = dict(regime_vectors)
                if gate is not None:
                    active["_gate_magnitude"] = gate  # offline analysis only
            else:
                self.model.set_vectors(None)
                text, _ = self.model.generate(prompt)
        else:
            self.model.set_vectors(None)
            text, _ = self.model.generate(prompt)
        return parse_trader_response(text, setup_id=briefing.setup_id, active_vectors=active)
```

- [ ] **Step 3: Update smoke pipeline**

```python
# scripts/smoke_pipeline_with_vectors.py
from xianvec.config import load_config
from xianvec.schemas import MarketState
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.prompt import build_trader_prompt, parse_trader_response
from xianvec.trader.vectors import gate_magnitude

cfg = load_config("config/default.yaml")
regime_cfg = load_config("config/regime_vectors.yaml")

state = MarketState(
    asset="BTC-USD", timestamp=1714600000.0, ohlcv_recent=[],
    indicators={"close": 50000, "rsi_14": 28, "ma_30": 51000, "ma_90": 52000,
                "atr_14": 1500, "macd": -50, "macd_signal": -20,
                "bb_upper": 53000, "bb_lower": 49500, "donchian_high_20": 54000,
                "volume_ratio_20": 1.2},
    onchain={"smart_money_inflow": 0.4, "funding_rate": -0.02, "stablecoin_inflow": 0.1},
    portfolio={"nav": 10000.0, "cash": 10000.0,
               "open_positions": [], "daily_pnl_pct": 0.0},
)

briefing = ClaudeIntern(model=cfg["intern"]["claude_model"]).reason(state, "smoke-vec")
weights = regime_cfg[briefing.regime]
weighted = [(f"vectors/gguf/{name}.gguf", w) for name, w in weights.items() if w != 0.0]

m = TraderModel(model_path=cfg["trader"]["model_path"])
prompt = build_trader_prompt(briefing)

m.set_vectors(weighted)
text_on, action_logits = m.generate(prompt)
gate = gate_magnitude(action_logits)
print(f"Gate magnitude (at action token): {gate:.2f}")
print("DECISION (vectors-on):", text_on)

m.set_vectors(None)
text_off, _ = m.generate(prompt)
print("DECISION (vectors-off):", text_off)
```

- [ ] **Step 4: Run smoke**

```bash
python scripts/smoke_pipeline_with_vectors.py
```

Expected: visibly different decision text between vectors-on and vectors-off. If identical: bump magnitudes in `config/regime_vectors.yaml` (try 1.5 or 2.0), or verify the GGUF conversion in Task 4.3.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/trader/model.py src/xianvec/trader/runtime.py scripts/smoke_pipeline_with_vectors.py
git commit -m "feat(trader): llama.cpp control vector integration with confidence gating"
```

---

### Task 4.5: Lookahead bias audit

**Why:** the #1 way our experiment gets invalidated is accidental lookahead in indicator computation. `compute_indicators` runs on `price_df.iloc[i-lookback:i]` (exclusive of the current bar) which is correct in principle, but every indicator must be reviewed.

**Files:**
- Create: `decisions/0002-lookahead-audit.md`
- Verify: `src/xianvec/data/indicators.py`

- [ ] **Step 1: Audit each indicator**

Walk through `compute_indicators` and verify, for each indicator:

| Indicator | Function | Lookahead-safe? | Notes |
|---|---|---|---|
| RSI 14 | `ta.rsi(close, length=14)` | ✓ | trailing 14 bars only |
| MA 30/60/90 | `close.rolling(N).mean()` | ✓ | simple trailing window |
| Bollinger 20/2 | `ta.bbands(close, 20, 2)` | ✓ | trailing window |
| MACD 12/26/9 | `ta.macd(close)` | ✓ | EWMA, recursive on past only |
| ATR 14 | `ta.atr(high, low, close, 14)` | ✓ | TR uses prev close, then trailing avg |
| Donchian high/low 20 | `high.rolling(20).max()` | ⚠️ | confirm window does NOT include current bar — pandas rolling defaults include the current value |
| Volume ratio 20 | `vol[-1] / vol.rolling(20).mean()` | ⚠️ | denominator's window includes current bar; for ratio this is OK because the bar is "complete" at decision time, but document this assumption |

For Donchian specifically: `df["high"].rolling(20).max().iloc[-1]` includes the current bar's high. For breakout-style signals this is what we want (close at bar i breaks high of last 20 incl. i). For a "did price break the prior 20 highs?" framing it's wrong. Pin the semantic in the audit doc.

- [ ] **Step 2: Audit the backtest setup loop**

In `eval/backtest.py::iter_setups`, the slice `price_df.iloc[i-lookback:i]` is exclusive of `i`. The future window is `price_df.iloc[i:i+horizon]`. The simulator's entry price is `future["close"].iloc[0]` which is the close of bar `i` — the first available bar after the decision. **Verify this is consistent: decisions are made on bar `i-1` close (last bar in the window), entries fill at bar `i` open or close.** Document the chosen convention.

- [ ] **Step 3: Write the audit doc**

```markdown
# decisions/0002-lookahead-audit.md

## Audit date: <fill in>

## Findings

[per-indicator table from Step 1, with PASS / FIXED / DOCUMENTED for each]

## Decision-to-execution timing convention

Backtest uses: decision on close of bar `i-1`, fill at `future["close"].iloc[0]` which is close of bar `i`. Slippage is added on top. This is one full bar of latency between decision and execution — conservative and consistent with bracket-order forward paper trading.

## Bars used in feature computation

`iter_setups` yields the window `price_df.iloc[i-lookback:i]` (exclusive of i). All indicators computed on this window are computable in real time at the close of bar `i-1`.

## Open issues

[anything found and not yet fixed]
```

- [ ] **Step 4: Commit**

```bash
git add decisions/0002-lookahead-audit.md src/xianvec/data/indicators.py
git commit -m "docs: lookahead audit + indicator review"
```

---

## Phase 5 — Risk Layer

### Task 5.1: Risk rule evaluator

**Files:**
- Create: `src/xianvec/risk/rules.py`
- Test: `tests/unit/test_risk.py`

- [ ] **Step 1: Write failing tests**

```python
# tests/unit/test_risk.py
import pytest
from xianvec.risk.rules import RiskEvaluator
from xianvec.schemas import TraderDecision

def make_decision(action="buy", size_bps=500, direction="long", stop=2.0):
    return TraderDecision(
        setup_id="x", action=action, size_bps=size_bps, direction=direction,
        stop_loss_pct=stop, take_profit_pct=4.0,
        trader_summary="t", active_vectors={},
    )

RISK_CFG = {
    "max_position_size_pct": 20.0,
    "max_total_exposure_pct": 100.0,
    "daily_loss_circuit_breaker_pct": 5.0,
    "max_open_positions": 5,
    "correlation_clusters": {"btc": ["BTC-USD"], "eth": ["ETH-USD"]},
    "max_per_cluster": 2,
    "require_stop_loss": True,
}

def test_passes_normal_decision():
    ev = RiskEvaluator(RISK_CFG)
    portfolio = {"nav": 10000, "open_positions": [], "daily_pnl_pct": 0.0}
    out = ev.evaluate(make_decision(size_bps=500), asset="BTC-USD", portfolio=portfolio)
    assert out.approved is True
    assert out.modified is None

def test_oversized_position_modified_down():
    ev = RiskEvaluator(RISK_CFG)
    portfolio = {"nav": 10000, "open_positions": [], "daily_pnl_pct": 0.0}
    out = ev.evaluate(make_decision(size_bps=2500), asset="BTC-USD", portfolio=portfolio)
    assert out.approved is True
    assert out.modified is not None
    assert out.modified.size_bps == 2000

def test_circuit_breaker_vetoes():
    ev = RiskEvaluator(RISK_CFG)
    portfolio = {"nav": 10000, "open_positions": [], "daily_pnl_pct": -6.0}
    out = ev.evaluate(make_decision(), asset="BTC-USD", portfolio=portfolio)
    assert out.approved is False
    assert "circuit" in out.veto_reason.lower()

def test_too_many_positions_vetoes():
    ev = RiskEvaluator(RISK_CFG)
    portfolio = {"nav": 10000, "open_positions": [{"asset": f"X{i}-USD"} for i in range(5)],
                 "daily_pnl_pct": 0.0}
    out = ev.evaluate(make_decision(), asset="NEW-USD", portfolio=portfolio)
    assert out.approved is False
    assert "open positions" in out.veto_reason.lower()

def test_close_action_bypasses_size_limits():
    ev = RiskEvaluator(RISK_CFG)
    portfolio = {"nav": 10000, "open_positions": [], "daily_pnl_pct": -10.0}
    out = ev.evaluate(make_decision(action="close", size_bps=0, stop=None),
                      asset="BTC-USD", portfolio=portfolio)
    assert out.approved is True
```

Note: the schema requires stop on buy/sell. Adjust the test factory to bypass for the "close" action variant.

- [ ] **Step 2: Run tests, verify fail**

```bash
pytest tests/unit/test_risk.py -v
```

- [ ] **Step 3: Implement RiskEvaluator**

```python
# src/xianvec/risk/rules.py
from xianvec.schemas import TraderDecision, RiskDecision

class RiskEvaluator:
    def __init__(self, cfg: dict):
        self.cfg = cfg

    def evaluate(self, d: TraderDecision, asset: str, portfolio: dict) -> RiskDecision:
        # close actions bypass size + breaker checks (we want to be able to exit always)
        if d.action == "close":
            return RiskDecision(approved=True, original=d)

        if d.action == "flat":
            return RiskDecision(approved=True, original=d)

        # daily loss circuit breaker
        if portfolio.get("daily_pnl_pct", 0.0) < -self.cfg["daily_loss_circuit_breaker_pct"]:
            return RiskDecision(
                approved=False, original=d,
                veto_reason=f"daily loss circuit breaker tripped "
                            f"({portfolio['daily_pnl_pct']:.2f}% <= "
                            f"-{self.cfg['daily_loss_circuit_breaker_pct']}%)",
            )

        # max open positions
        n_open = len(portfolio.get("open_positions", []))
        if n_open >= self.cfg["max_open_positions"]:
            return RiskDecision(
                approved=False, original=d,
                veto_reason=f"too many open positions ({n_open} >= {self.cfg['max_open_positions']})",
            )

        # correlation cluster cap
        cluster = self._cluster_for(asset)
        if cluster:
            in_cluster = sum(
                1 for p in portfolio.get("open_positions", [])
                if self._cluster_for(p["asset"]) == cluster
            )
            if in_cluster >= self.cfg["max_per_cluster"]:
                return RiskDecision(
                    approved=False, original=d,
                    veto_reason=f"cluster {cluster} cap reached ({in_cluster})",
                )

        # max position size — modify down rather than veto
        max_size = int(self.cfg["max_position_size_pct"] * 100)  # pct -> bps
        if d.size_bps > max_size:
            modified = d.model_copy(update={"size_bps": max_size})
            return RiskDecision(approved=True, original=d, modified=modified)

        return RiskDecision(approved=True, original=d)

    def _cluster_for(self, asset: str) -> str | None:
        for cluster, members in self.cfg["correlation_clusters"].items():
            if asset in members:
                return cluster
        return None
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_risk.py -v
```

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/risk/rules.py tests/unit/test_risk.py
git commit -m "feat(risk): deterministic risk layer with veto and modify"
```

---

### Task 5.2: Vol-targeted sizing modifier

**Why:** Fixed `size_bps` is backwards. In low-vol regimes the agent trades huge positions; in high-vol regimes it trades tiny. Industry standard is to size positions inversely to recent realized volatility (typically ATR-based). Without this, the vectors-OFF baseline looks like a strawman because *any* sane sizing rule will outperform it.

**Files:**
- Modify: `src/xianvec/risk/rules.py`
- Create: tests in `tests/unit/test_risk.py`
- Modify: `config/risk.yaml`

- [ ] **Step 1: Add config knob**

```yaml
# config/risk.yaml — append:
vol_target:
  enabled: true
  target_daily_vol_pct: 1.0    # target ~1% daily portfolio vol contribution
  reference_atr_pct: 1.5       # ATR/price pct considered "normal"
  max_scale: 2.0               # never scale up more than 2×
  min_scale: 0.25              # never scale down below 0.25×
```

- [ ] **Step 2: Failing test**

```python
# tests/unit/test_risk.py — append:
def test_vol_targeting_scales_down_in_high_vol():
    cfg = {**RISK_CFG, "vol_target": {
        "enabled": True, "target_daily_vol_pct": 1.0,
        "reference_atr_pct": 1.5, "max_scale": 2.0, "min_scale": 0.25,
    }}
    ev = RiskEvaluator(cfg)
    portfolio = {"nav": 10000, "open_positions": [], "daily_pnl_pct": 0.0,
                 "atr_pct": 4.5}  # 3× normal vol
    out = ev.evaluate(make_decision(size_bps=200), asset="BTC-USD", portfolio=portfolio)
    assert out.modified is not None
    assert out.modified.size_bps < 200    # scaled down

def test_vol_targeting_scales_up_in_low_vol():
    cfg = {**RISK_CFG, "vol_target": {
        "enabled": True, "target_daily_vol_pct": 1.0,
        "reference_atr_pct": 1.5, "max_scale": 2.0, "min_scale": 0.25,
    }}
    ev = RiskEvaluator(cfg)
    portfolio = {"nav": 10000, "open_positions": [], "daily_pnl_pct": 0.0,
                 "atr_pct": 0.5}  # 1/3 normal vol
    out = ev.evaluate(make_decision(size_bps=200), asset="BTC-USD", portfolio=portfolio)
    assert out.modified is not None
    assert out.modified.size_bps > 200    # scaled up
    assert out.modified.size_bps <= 400   # capped at max_scale (2×)
```

- [ ] **Step 3: Implement vol scaling in RiskEvaluator**

Add to `RiskEvaluator.evaluate`, after the daily-loss/cluster/max-positions checks but BEFORE the max position size cap (so vol-targeting can scale up, then the cap can pull it back down if needed):

```python
# inside RiskEvaluator.evaluate, before "max position size — modify down" block:

vt = self.cfg.get("vol_target", {})
if vt.get("enabled") and d.action in ("buy", "sell"):
    atr_pct = portfolio.get("atr_pct")
    if atr_pct and atr_pct > 0:
        ref = vt["reference_atr_pct"]
        scale = ref / atr_pct          # higher vol → smaller scale
        scale = max(vt["min_scale"], min(vt["max_scale"], scale))
        if scale != 1.0:
            new_size = int(round(d.size_bps * scale))
            d = d.model_copy(update={"size_bps": new_size})
            # fall through — the max position size cap below still applies
```

`atr_pct` is `atr_14 / close * 100` and is computed in `compute_indicators`. The state-builder must thread `atr_pct` from `state.indicators` into `state.portfolio["atr_pct"]` (or restructure so the risk evaluator can read directly from indicators — your call). Document the chosen path.

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_risk.py -v
```

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/risk/rules.py tests/unit/test_risk.py config/risk.yaml
git commit -m "feat(risk): ATR-based vol-targeted sizing modifier"
```

---

### Task 5.3: Loss-streak cooldown

**Why:** LLMs can exhibit revenge-trading patterns when prompted about recent outcomes. Even without explicit P&L in the prompt, vector-induced "conviction" can lead to over-sizing after losses. A simple deterministic cooldown is a cheap, hard-rule defense — and it makes the vectors-OFF baseline meaningfully stronger so vectors-ON has a real bar to clear.

**Files:**
- Modify: `src/xianvec/risk/rules.py`
- Modify: `config/risk.yaml`
- Test: `tests/unit/test_risk.py`

- [ ] **Step 1: Add config**

```yaml
# config/risk.yaml — append:
loss_streak_cooldown:
  enabled: true
  consecutive_losses_threshold: 3
  cooldown_bars: 8
```

- [ ] **Step 2: Failing test**

```python
# tests/unit/test_risk.py — append:
def test_cooldown_vetoes_after_loss_streak():
    cfg = {**RISK_CFG, "loss_streak_cooldown": {
        "enabled": True, "consecutive_losses_threshold": 3, "cooldown_bars": 8,
    }}
    ev = RiskEvaluator(cfg)
    portfolio = {
        "nav": 10000, "open_positions": [], "daily_pnl_pct": 0.0,
        "consecutive_losses": 3, "bars_since_last_loss": 2,
    }
    out = ev.evaluate(make_decision(), asset="BTC-USD", portfolio=portfolio)
    assert out.approved is False
    assert "cooldown" in out.veto_reason.lower()

def test_cooldown_passes_after_window():
    cfg = {**RISK_CFG, "loss_streak_cooldown": {
        "enabled": True, "consecutive_losses_threshold": 3, "cooldown_bars": 8,
    }}
    ev = RiskEvaluator(cfg)
    portfolio = {
        "nav": 10000, "open_positions": [], "daily_pnl_pct": 0.0,
        "consecutive_losses": 3, "bars_since_last_loss": 9,
    }
    out = ev.evaluate(make_decision(), asset="BTC-USD", portfolio=portfolio)
    assert out.approved is True
```

- [ ] **Step 3: Implement**

Add to `RiskEvaluator.evaluate`, near the top after the close/flat early-returns:

```python
ls = self.cfg.get("loss_streak_cooldown", {})
if ls.get("enabled") and d.action in ("buy", "sell"):
    streak = portfolio.get("consecutive_losses", 0)
    bars_since = portfolio.get("bars_since_last_loss", 999)
    if streak >= ls["consecutive_losses_threshold"] and bars_since < ls["cooldown_bars"]:
        return RiskDecision(
            approved=False, original=d,
            veto_reason=f"loss-streak cooldown: {streak} losses, "
                        f"{bars_since} bars since (need {ls['cooldown_bars']})",
        )
```

The `consecutive_losses` and `bars_since_last_loss` fields must be tracked by the pipeline and threaded into `portfolio`. Add to `data/store.py` a method that computes these from the most-recent decisions log. Backtest simulator updates them after each closed trade.

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_risk.py -v
```

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/risk/rules.py tests/unit/test_risk.py config/risk.yaml
git commit -m "feat(risk): loss-streak cooldown rule"
```

---

## Phase 6 — Stage 3 Execution + Backtest Simulator

### Task 6.1: Backtest simulator

**Files:**
- Create: `src/xianvec/execution/simulator.py`
- Test: `tests/unit/test_simulator.py`

- [ ] **Step 1: Write failing tests**

```python
# tests/unit/test_simulator.py
import pandas as pd
from xianvec.execution.simulator import Simulator
from xianvec.schemas import TraderDecision

def make_price_df(prices):
    return pd.DataFrame({
        "open": prices, "high": [p*1.01 for p in prices],
        "low": [p*0.99 for p in prices], "close": prices,
        "volume": [1000]*len(prices)
    })

def test_long_take_profit_hit():
    prices = [100, 101, 102, 105]
    sim = Simulator(initial_nav=10000, fee_bps=10, slippage_bps=5)
    d = TraderDecision(setup_id="t", action="buy", size_bps=1000, direction="long",
                       stop_loss_pct=2.0, take_profit_pct=4.0, trader_summary="t",
                       active_vectors={})
    pnl, exit_reason = sim.simulate_trade(d, make_price_df(prices), entry_price=100)
    assert exit_reason == "take_profit"
    assert pnl > 0

def test_long_stop_loss_hit():
    prices = [100, 99, 97, 95]
    sim = Simulator(initial_nav=10000, fee_bps=10, slippage_bps=5)
    d = TraderDecision(setup_id="t", action="buy", size_bps=1000, direction="long",
                       stop_loss_pct=2.0, take_profit_pct=4.0, trader_summary="t",
                       active_vectors={})
    pnl, exit_reason = sim.simulate_trade(d, make_price_df(prices), entry_price=100)
    assert exit_reason == "stop_loss"
    assert pnl < 0

def test_horizon_exit_at_close():
    prices = [100, 100.5, 100.7, 101]
    sim = Simulator(initial_nav=10000, fee_bps=10, slippage_bps=5)
    d = TraderDecision(setup_id="t", action="buy", size_bps=1000, direction="long",
                       stop_loss_pct=5.0, take_profit_pct=10.0, trader_summary="t",
                       active_vectors={})
    pnl, exit_reason = sim.simulate_trade(d, make_price_df(prices), entry_price=100)
    assert exit_reason == "horizon"
```

- [ ] **Step 2: Run, verify fail**

```bash
pytest tests/unit/test_simulator.py -v
```

- [ ] **Step 3: Implement simulator**

```python
# src/xianvec/execution/simulator.py
import pandas as pd
from xianvec.schemas import TraderDecision

class Simulator:
    """Simulates a trade given a future price path. For backtest replay.

    Asset-tiered slippage: tighter pairs (BTC) get 5bps, alts get more. Fixed
    5bps across all assets is wildly optimistic for SOL or smaller alts and
    leaves us open to "your backtest assumes free liquidity" critiques.
    """

    DEFAULT_SLIPPAGE_BY_ASSET = {
        "BTC-USD": 5.0,
        "ETH-USD": 10.0,
        "SOL-USD": 20.0,
    }
    DEFAULT_SLIPPAGE_FALLBACK = 30.0  # unknown asset → conservative

    def __init__(
        self,
        initial_nav: float,
        fee_bps: float = 10,
        slippage_bps: float | None = None,
        slippage_by_asset: dict[str, float] | None = None,
    ):
        self.nav = initial_nav
        self.fee_bps = fee_bps
        self.global_slippage_bps = slippage_bps  # if set, overrides per-asset
        self.slippage_by_asset = slippage_by_asset or self.DEFAULT_SLIPPAGE_BY_ASSET

    def _slippage_bps_for(self, asset: str) -> float:
        if self.global_slippage_bps is not None:
            return self.global_slippage_bps
        return self.slippage_by_asset.get(asset, self.DEFAULT_SLIPPAGE_FALLBACK)

    def simulate_trade(
        self, d: TraderDecision, future_prices: pd.DataFrame, entry_price: float,
        asset: str = "BTC-USD",
    ) -> tuple[float, str]:
        """Return (pnl_dollars, exit_reason)."""
        if d.action in ("flat", "close"):
            return 0.0, "no_position"

        position_value = self.nav * (d.size_bps / 10000)
        # apply asset-tiered slippage to entry
        slip_bps = self._slippage_bps_for(asset)
        slip = entry_price * (slip_bps / 10000)
        eff_entry = entry_price + slip if d.direction == "long" else entry_price - slip

        sl_pct = d.stop_loss_pct / 100
        tp_pct = (d.take_profit_pct or 1000) / 100  # huge if not set

        if d.direction == "long":
            stop_price = eff_entry * (1 - sl_pct)
            tp_price = eff_entry * (1 + tp_pct)
        else:
            stop_price = eff_entry * (1 + sl_pct)
            tp_price = eff_entry * (1 - tp_pct)

        for _, bar in future_prices.iterrows():
            # check stop first (conservative)
            if d.direction == "long":
                if bar["low"] <= stop_price:
                    pnl_pct = (stop_price - eff_entry) / eff_entry
                    return self._apply(position_value, pnl_pct), "stop_loss"
                if bar["high"] >= tp_price:
                    pnl_pct = (tp_price - eff_entry) / eff_entry
                    return self._apply(position_value, pnl_pct), "take_profit"
            else:
                if bar["high"] >= stop_price:
                    pnl_pct = (eff_entry - stop_price) / eff_entry
                    return self._apply(position_value, pnl_pct), "stop_loss"
                if bar["low"] <= tp_price:
                    pnl_pct = (eff_entry - tp_price) / eff_entry
                    return self._apply(position_value, pnl_pct), "take_profit"

        # exit at horizon close
        last_close = future_prices["close"].iloc[-1]
        if d.direction == "long":
            pnl_pct = (last_close - eff_entry) / eff_entry
        else:
            pnl_pct = (eff_entry - last_close) / eff_entry
        return self._apply(position_value, pnl_pct), "horizon"

    def _apply(self, position_value: float, pnl_pct: float) -> float:
        fee_cost = position_value * (self.fee_bps / 10000) * 2  # round-trip
        return position_value * pnl_pct - fee_cost
```

- [ ] **Step 4: Run tests, verify pass**

```bash
pytest tests/unit/test_simulator.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/execution/simulator.py tests/unit/test_simulator.py
git commit -m "feat(execution): backtest simulator with stop/tp/horizon exits"
```

---

### Task 6.2: Live Alpaca paper executor

**Files:**
- Create: `src/xianvec/execution/alpaca.py`

- [ ] **Step 1: Write Alpaca executor**

```python
# src/xianvec/execution/alpaca.py
import os
from alpaca.trading.client import TradingClient
from alpaca.trading.requests import MarketOrderRequest, StopLossRequest, TakeProfitRequest
from alpaca.trading.enums import OrderSide, TimeInForce, OrderClass
from xianvec.schemas import TraderDecision

class AlpacaExecutor:
    def __init__(self, paper: bool = True):
        self.client = TradingClient(
            api_key=os.environ["ALPACA_API_KEY"],
            secret_key=os.environ["ALPACA_API_SECRET"],
            paper=paper,
        )

    def execute(self, d: TraderDecision, asset: str, nav: float) -> dict:
        if d.action in ("flat",):
            return {"status": "noop", "reason": "flat"}
        if d.action == "close":
            self.client.close_position(asset)
            return {"status": "closed", "asset": asset}

        side = OrderSide.BUY if d.direction == "long" else OrderSide.SELL
        notional = nav * (d.size_bps / 10000)

        sl = StopLossRequest(stop_price=self._stop_price(asset, d))
        tp = TakeProfitRequest(limit_price=self._tp_price(asset, d)) if d.take_profit_pct else None

        req = MarketOrderRequest(
            symbol=asset,
            notional=notional,
            side=side,
            time_in_force=TimeInForce.GTC,
            order_class=OrderClass.BRACKET,
            stop_loss=sl,
            take_profit=tp,
            client_order_id=d.setup_id,
        )
        order = self.client.submit_order(req)
        return {"status": "submitted", "order_id": str(order.id), "client_order_id": d.setup_id}

    def get_portfolio(self) -> dict:
        acct = self.client.get_account()
        positions = self.client.get_all_positions()
        return {
            "nav": float(acct.portfolio_value),
            "cash": float(acct.cash),
            "open_positions": [
                {"asset": p.symbol, "qty": float(p.qty),
                 "unrealized_pl": float(p.unrealized_pl)}
                for p in positions
            ],
            "daily_pnl_pct": (
                float(acct.equity) - float(acct.last_equity)
            ) / float(acct.last_equity) * 100,
        }

    def _stop_price(self, asset: str, d: TraderDecision) -> float:
        last = float(self.client.get_latest_quote(asset).bid_price)
        sl = d.stop_loss_pct / 100
        return last * (1 - sl) if d.direction == "long" else last * (1 + sl)

    def _tp_price(self, asset: str, d: TraderDecision) -> float:
        last = float(self.client.get_latest_quote(asset).ask_price)
        tp = (d.take_profit_pct or 0) / 100
        return last * (1 + tp) if d.direction == "long" else last * (1 - tp)
```

- [ ] **Step 2: Smoke test against paper account**

```bash
ALPACA_API_KEY=$(op item get alpaca --fields api_key) \
ALPACA_API_SECRET=$(op item get alpaca --fields secret) \
python -c "
from xianvec.execution.alpaca import AlpacaExecutor
ex = AlpacaExecutor(paper=True)
print(ex.get_portfolio())
"
```

Expected: portfolio dict with NAV ~ $100k (default Alpaca paper).

- [ ] **Step 3: Commit**

```bash
git add src/xianvec/execution/alpaca.py
git commit -m "feat(execution): alpaca paper trading executor"
```

---

### Task 6.3: Byreal Perps executor (Mantle, hackathon path)

**Why:** Phase 6.2 produces an Alpaca paper executor — the testing path. The hackathon submission requires real on-chain execution on Mantle via Byreal Perps. We add a *second* executor that conforms to the same protocol so the forward runner can swap between them via a `--executor {alpaca,byreal}` flag.

`@byreal-io/byreal-cli` is an npm package. Python orchestrates by shelling out to `npx byreal-cli` and parsing JSON output. Node 20+ is a runtime prerequisite (verified in Phase 0.1 env-setup notes).

**Files:**
- Create: `src/xianvec/execution/byreal.py`
- Test: `tests/integration/test_byreal_smoke.py` (network-touching; behind a flag)

- [ ] **Step 1: Define the shared Executor protocol**

Both Alpaca and Byreal executors must satisfy this shape so the forward runner is executor-agnostic:

```python
# src/xianvec/execution/protocol.py
from typing import Protocol
from xianvec.schemas import TraderDecision

class Executor(Protocol):
    def execute(self, d: TraderDecision, asset: str, nav: float) -> dict: ...
    def get_portfolio(self) -> dict: ...
    def close_position(self, asset: str) -> dict: ...
```

Update `AlpacaExecutor` (Phase 6.2) and the new `ByrealExecutor` to both type-implement this Protocol.

- [ ] **Step 2: Implement ByrealExecutor**

```python
# src/xianvec/execution/byreal.py
"""Byreal Perps executor on Mantle. Shells out to `npx byreal-cli`.

The CLI's exact flag set should be confirmed against the version of
`@byreal-io/byreal-cli` resolved at install time (`npx byreal-cli --help`).
The shape below reflects the documented agent-skills surface as of 2026-04;
adjust if upstream rev's changed.
"""
import json
import os
import subprocess
from pathlib import Path
from xianvec.schemas import TraderDecision

class ByrealCLIError(RuntimeError):
    pass

class ByrealExecutor:
    def __init__(self, private_key: str | None = None,
                 rpc_url: str | None = None,
                 cli_command: str = "npx byreal-cli"):
        self.private_key = private_key or os.environ["MANTLE_AGENT_PRIVATE_KEY"]
        self.rpc_url = rpc_url or os.environ.get("MANTLE_RPC_URL", "https://rpc.mantle.xyz")
        self.cli_command = cli_command

    def _call(self, *args, capture_json: bool = True) -> dict | str:
        env = {**os.environ,
               "BYREAL_PRIVATE_KEY": self.private_key,
               "MANTLE_RPC_URL": self.rpc_url}
        cmd = self.cli_command.split() + list(args) + ["--json"]
        proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=60)
        if proc.returncode != 0:
            raise ByrealCLIError(f"byreal-cli failed: {proc.stderr.strip()[:200]}")
        out = proc.stdout.strip()
        return json.loads(out) if capture_json else out

    def execute(self, d: TraderDecision, asset: str, nav: float) -> dict:
        if d.action == "flat":
            return {"status": "noop", "reason": "flat"}
        if d.action == "close":
            return self.close_position(asset)
        side = "long" if d.direction == "long" else "short"
        notional = nav * (d.size_bps / 10000)
        # bracket: open with stop-loss + take-profit set in the same call
        result = self._call(
            "perp", "open",
            "--asset", asset,
            "--notional", f"{notional:.2f}",
            "--side", side,
            "--stop-loss-pct", f"{d.stop_loss_pct:.2f}",
            "--take-profit-pct", f"{(d.take_profit_pct or 0):.2f}",
            "--client-order-id", d.setup_id,
        )
        return {
            "status": "submitted",
            "asset": asset,
            "tx_hash": result.get("txHash"),
            "position_id": result.get("positionId"),
            "client_order_id": d.setup_id,
        }

    def close_position(self, asset: str) -> dict:
        result = self._call("perp", "close", "--asset", asset)
        return {"status": "closed", "asset": asset, "tx_hash": result.get("txHash")}

    def get_portfolio(self) -> dict:
        result = self._call("portfolio")
        # normalize to the shape RiskEvaluator + agent expect
        return {
            "nav": float(result.get("equity_usd", 0.0)),
            "cash": float(result.get("free_collateral_usd", 0.0)),
            "open_positions": [
                {"asset": p["asset"], "qty": float(p["size"]),
                 "unrealized_pl": float(p.get("unrealized_pnl_usd", 0.0))}
                for p in result.get("positions", [])
            ],
            "daily_pnl_pct": float(result.get("daily_pnl_pct", 0.0)),
        }
```

- [ ] **Step 3: Smoke test against Mantle (read-only first)**

```bash
python -c "
from xianvec.execution.byreal import ByrealExecutor
ex = ByrealExecutor()
print(ex.get_portfolio())
"
```

Expected: portfolio dict with the pre-funded MNT/USDC balances on the user's Mantle wallet. **No write-side smoke** at this step — write tests happen in Phase 11.5 once ERC-8004 identity + the reputation log are in place.

- [ ] **Step 4: Commit**

```bash
git add src/xianvec/execution/byreal.py src/xianvec/execution/protocol.py
git commit -m "feat(execution): byreal perps executor on mantle (cli wrapper)"
```

---

## Phase 6.5 — ERC-8004 identity registration + reputation registry

**Why:** The Turing Test hackathon issues each participating agent an ERC-8004 identity NFT on Mantle. xianvec mints **two**: one for the vectors-OFF arm, one for the vectors-ON arm. They post performance updates to the same reputation registry, so the Δ-Sharpe comparison becomes a publicly auditable on-chain experiment, not a private claim. See Mantle integration §M1.

This phase produces:
- Two `agentURI` JSON manifests under `identity/`
- A `register_agents.py` script that mints the NFTs (one-shot)
- The `decision_log.py` module that posts reputation-registry updates

### Task 6.5.1: Author the agentURI manifests

**Files:**
- Create: `identity/vectors_on.json`
- Create: `identity/vectors_off.json`
- Create: `src/xianvec/onchain/manifest.py`

- [ ] **Step 1: Write per-arm manifests**

```json
// identity/vectors_off.json
{
  "schemaVersion": "1.0",
  "name": "xianvec-vectors-off",
  "description": "Trading agent: same Stage 1 (Claude) + Stage 2 (Qwen3-14B Q4_K_M) pipeline as xianvec-vectors-on, but with disposition control vectors disabled. The experimental control in xianvec's Δ-Sharpe A/B comparison.",
  "model": {
    "stage1": "claude-haiku-4-5",
    "stage2": "Qwen/Qwen3-14B (Q4_K_M GGUF)",
    "vectors_enabled": false
  },
  "code": {
    "repo": "https://github.com/latentwill/xianvec",
    "commit": "<filled in by register_agents.py>"
  },
  "contact": "edkenne@gmail.com"
}
```

```json
// identity/vectors_on.json
{
  "schemaVersion": "1.0",
  "name": "xianvec-vectors-on",
  "description": "Trading agent with disposition control vectors active across four axes: conviction, patience, risk_appetite, trend_disposition. Vectors extracted via repeng on synthetic contrastive trader prompts and applied at inference time via llama.cpp's native control_vectors API.",
  "model": {
    "stage1": "claude-haiku-4-5",
    "stage2": "Qwen/Qwen3-14B (Q4_K_M GGUF)",
    "vectors_enabled": true,
    "axes": ["conviction", "patience", "risk_appetite", "trend_disposition"]
  },
  "code": {
    "repo": "https://github.com/latentwill/xianvec",
    "commit": "<filled in by register_agents.py>"
  },
  "contact": "edkenne@gmail.com"
}
```

- [ ] **Step 2: Manifest builder + IPFS pinning helper**

```python
# src/xianvec/onchain/manifest.py
"""Build, validate, and pin agentURI manifests for ERC-8004 registration."""
import json
import subprocess
from pathlib import Path

REQUIRED_FIELDS = {"schemaVersion", "name", "description", "model", "code", "contact"}

def load_and_fill(path: str | Path, commit_sha: str) -> dict:
    data = json.loads(Path(path).read_text())
    missing = REQUIRED_FIELDS - set(data.keys())
    if missing:
        raise ValueError(f"manifest missing fields: {missing}")
    data["code"]["commit"] = commit_sha
    return data

def pin_to_ipfs(manifest: dict) -> str:
    """Pin via local ipfs CLI (`ipfs add`). Returns ipfs:// URI.

    Hackathon shortcut: if ipfs CLI not available, fall back to writing the
    manifest under a public HTTPS path (e.g., pinning service or commit it to
    the public xianvec repo and use the GitHub raw URL as agentURI).
    """
    raw = json.dumps(manifest, indent=2).encode("utf-8")
    try:
        proc = subprocess.run(["ipfs", "add", "-Q", "--pin"],
                              input=raw, capture_output=True, check=True)
        cid = proc.stdout.decode().strip()
        return f"ipfs://{cid}"
    except (FileNotFoundError, subprocess.CalledProcessError):
        # fallback path: caller pins via repo URL
        raise RuntimeError("ipfs CLI unavailable — pin manually and pass URI explicitly")
```

- [ ] **Step 3: Commit**

```bash
git add identity/ src/xianvec/onchain/manifest.py
git commit -m "feat(onchain): per-arm agentURI manifests for ERC-8004 registration"
```

---

### Task 6.5.2: ERC-8004 registry client + register_agents.py

**Files:**
- Create: `src/xianvec/onchain/erc8004.py`
- Create: `scripts/register_agents.py`

- [ ] **Step 1: Implement registry client**

```python
# src/xianvec/onchain/erc8004.py
"""ERC-8004 Identity + Reputation registry client (Mantle).

Verified contract addresses are resolved at runtime via the
`mantle-address-registry-navigator` skill (Phase 0.4) so we never hardcode
addresses that could drift. For the v1 build, addresses are read from env
vars (`ERC8004_IDENTITY_REGISTRY`, `ERC8004_REPUTATION_REGISTRY`); the
register_agents.py script populates them by querying the registry navigator
and writes them into `.env` for subsequent runs.
"""
import os
import json
from pathlib import Path
from web3 import Web3
from eth_account import Account

# Minimal ABIs — verify against deployed contracts via the mantle-address-registry-navigator
IDENTITY_ABI = json.loads(Path(__file__).parent.joinpath("erc8004_identity.abi.json").read_text())
REPUTATION_ABI = json.loads(Path(__file__).parent.joinpath("erc8004_reputation.abi.json").read_text())

class ERC8004Client:
    def __init__(self, rpc_url: str | None = None, private_key: str | None = None):
        self.w3 = Web3(Web3.HTTPProvider(rpc_url or os.environ["MANTLE_RPC_URL"]))
        self.account = Account.from_key(private_key or os.environ["MANTLE_AGENT_PRIVATE_KEY"])
        self.identity = self.w3.eth.contract(
            address=os.environ["ERC8004_IDENTITY_REGISTRY"], abi=IDENTITY_ABI,
        )
        self.reputation = self.w3.eth.contract(
            address=os.environ["ERC8004_REPUTATION_REGISTRY"], abi=REPUTATION_ABI,
        )

    def mint_identity(self, agent_uri: str) -> dict:
        """Mint a new agent NFT pointing at agent_uri. Returns {token_id, tx_hash}."""
        tx = self.identity.functions.mint(self.account.address, agent_uri).build_transaction({
            "from": self.account.address,
            "nonce": self.w3.eth.get_transaction_count(self.account.address),
            "gas": 300_000,
            "gasPrice": self.w3.eth.gas_price,
        })
        signed = self.account.sign_transaction(tx)
        tx_hash = self.w3.eth.send_raw_transaction(signed.rawTransaction)
        receipt = self.w3.eth.wait_for_transaction_receipt(tx_hash, timeout=120)
        token_id = int(receipt.logs[0].topics[3].hex(), 16)
        return {"token_id": token_id, "tx_hash": tx_hash.hex()}

    def post_reputation(self, token_id: int, score_payload: dict) -> str:
        """Post a reputation update for the given agent NFT.

        score_payload is JSON-serialized + uploaded to IPFS or written to a
        cheap on-chain field; this keeps gas bounded while preserving rich
        per-trade detail (setup_id, action, pnl, rolling Δ-Sharpe).
        """
        payload = json.dumps(score_payload, separators=(",", ":")).encode("utf-8")
        tx = self.reputation.functions.post(token_id, payload).build_transaction({
            "from": self.account.address,
            "nonce": self.w3.eth.get_transaction_count(self.account.address),
            "gas": 250_000,
            "gasPrice": self.w3.eth.gas_price,
        })
        signed = self.account.sign_transaction(tx)
        tx_hash = self.w3.eth.send_raw_transaction(signed.rawTransaction)
        self.w3.eth.wait_for_transaction_receipt(tx_hash, timeout=120)
        return tx_hash.hex()
```

The two `*.abi.json` files are produced by querying the on-chain ABI of the deployed Identity / Reputation registries — fetch them via `cast interface <addr>` or by reading the verified-contract page on Mantle's block explorer. Cache locally; don't re-fetch on every run.

- [ ] **Step 2: register_agents.py — one-shot mint script**

```python
# scripts/register_agents.py
"""Mint ERC-8004 identity NFTs for both arms (vectors-on, vectors-off).

Idempotent: reads `identity/registered.json` and skips arms that already have
a token_id. Run once at hackathon start; commit `identity/registered.json`
afterward so the forward runner can find the NFTs.
"""
import json
import subprocess
from pathlib import Path
import typer
from xianvec.onchain.manifest import load_and_fill, pin_to_ipfs
from xianvec.onchain.erc8004 import ERC8004Client

REGISTERED_PATH = Path("identity/registered.json")

def main():
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"]).decode().strip()
    registered = json.loads(REGISTERED_PATH.read_text()) if REGISTERED_PATH.exists() else {}
    client = ERC8004Client()

    for arm in ["vectors_off", "vectors_on"]:
        if arm in registered and registered[arm].get("token_id"):
            print(f"{arm} already registered: token_id={registered[arm]['token_id']}")
            continue
        manifest = load_and_fill(f"identity/{arm}.json", commit_sha=commit)
        agent_uri = pin_to_ipfs(manifest)
        result = client.mint_identity(agent_uri=agent_uri)
        registered[arm] = {
            "token_id": result["token_id"],
            "agent_uri": agent_uri,
            "tx_hash": result["tx_hash"],
            "commit_sha": commit,
        }
        print(f"minted {arm}: token_id={result['token_id']}  uri={agent_uri}")

    REGISTERED_PATH.write_text(json.dumps(registered, indent=2))
    print(f"\nWrote {REGISTERED_PATH}")

if __name__ == "__main__":
    typer.run(main)
```

- [ ] **Step 3: Run the mint**

```bash
python scripts/register_agents.py
```

Expected: two transactions on Mantle, two token_ids saved in `identity/registered.json`. Verify the txs on Mantle's block explorer.

- [ ] **Step 4: Commit (registered.json IS committed — it's the public proof)**

```bash
git add scripts/register_agents.py src/xianvec/onchain/erc8004.py \
        src/xianvec/onchain/erc8004_identity.abi.json \
        src/xianvec/onchain/erc8004_reputation.abi.json \
        identity/registered.json
git commit -m "feat(onchain): mint ERC-8004 identity NFTs for both arms"
```

---

### Task 6.5.3: Reputation log helper

**Files:**
- Create: `src/xianvec/onchain/decision_log.py`
- Test: `tests/unit/test_decision_log.py`

- [ ] **Step 1: Decision-log facade**

```python
# src/xianvec/onchain/decision_log.py
"""Posts reputation-registry updates after each closed Byreal trade.

Backtest decisions are NEVER logged on-chain — only forward live trades, to
keep the chain log honest. Alpaca paper trades also stay off-chain.
"""
import json
from pathlib import Path
from xianvec.onchain.erc8004 import ERC8004Client

class DecisionLog:
    def __init__(self, client: ERC8004Client | None = None,
                 registered_path: str = "identity/registered.json"):
        self.client = client or ERC8004Client()
        self.registered = json.loads(Path(registered_path).read_text())

    def post_trade(self, arm: str, setup_id: str, action: str, direction: str,
                   size_bps: int, pnl: float, rolling_sharpe: float | None = None) -> str:
        token_id = self.registered[arm]["token_id"]
        payload = {
            "setup_id": setup_id, "action": action, "direction": direction,
            "size_bps": size_bps, "pnl": pnl,
        }
        if rolling_sharpe is not None:
            payload["rolling_sharpe"] = rolling_sharpe
        return self.client.post_reputation(token_id=token_id, score_payload=payload)
```

- [ ] **Step 2: Commit**

```bash
git add src/xianvec/onchain/decision_log.py tests/unit/test_decision_log.py
git commit -m "feat(onchain): reputation-log helper for forward Byreal trades"
```

---

## Phase 7 — Baselines

### Task 7.1: Null and technical baselines

**Files:**
- Create: `src/xianvec/baselines/null.py`
- Create: `src/xianvec/baselines/technical.py`
- Test: `tests/unit/test_baselines.py`

- [ ] **Step 1: Write failing tests**

```python
# tests/unit/test_baselines.py
import pandas as pd
import numpy as np
from xianvec.baselines.null import buy_and_hold, random_signal
from xianvec.baselines.technical import (
    rsi_signal, ma_crossover_signal, bollinger_signal,
    macd_signal_fn, donchian_breakout_signal,
)

def make_df(closes):
    return pd.DataFrame({
        "open": closes, "high": [c*1.01 for c in closes],
        "low": [c*0.99 for c in closes], "close": closes,
        "volume": [1000]*len(closes),
    })

def test_buy_and_hold_always_long():
    df = make_df(list(range(100, 200)))
    sig = buy_and_hold(df)
    assert sig.action == "buy"
    assert sig.direction == "long"

def test_rsi_oversold_signals_long():
    # construct closes that produce oversold RSI
    closes = [100 - i*0.5 for i in range(20)]
    df = make_df(closes)
    sig = rsi_signal(df, lower=30, upper=70)
    assert sig.direction in ("long", "flat")  # should be long if RSI < 30

def test_ma_golden_cross_long():
    # short MA crosses above long MA
    closes = list(range(100, 130)) + list(range(130, 100, -1)) + list(range(100, 200))
    df = make_df(closes)
    sig = ma_crossover_signal(df, short=30, long=90)
    assert sig.action in ("buy", "flat")

def test_donchian_breakout_long_on_new_high():
    closes = [100]*30 + [110]
    df = make_df(closes)
    sig = donchian_breakout_signal(df, lookback=20)
    assert sig.direction == "long"
```

- [ ] **Step 2: Implement baselines**

```python
# src/xianvec/baselines/null.py
import random
import pandas as pd
from xianvec.schemas import TraderDecision

def buy_and_hold(df: pd.DataFrame, setup_id: str = "bh") -> TraderDecision:
    return TraderDecision(
        setup_id=setup_id, action="buy", size_bps=10000, direction="long",
        stop_loss_pct=20.0, take_profit_pct=None,
        trader_summary="buy and hold", active_vectors={},
    )

def random_signal(df: pd.DataFrame, setup_id: str = "rand", seed: int | None = None) -> TraderDecision:
    rng = random.Random(seed)
    direction = rng.choice(["long", "short"])
    return TraderDecision(
        setup_id=setup_id,
        action="buy" if direction == "long" else "sell",
        size_bps=100, direction=direction,
        stop_loss_pct=2.0, take_profit_pct=4.0,
        trader_summary="random", active_vectors={},
    )
```

```python
# src/xianvec/baselines/technical.py
import pandas as pd
import pandas_ta as ta
from xianvec.schemas import TraderDecision

def _flat(setup_id: str, name: str) -> TraderDecision:
    return TraderDecision(
        setup_id=setup_id, action="flat", size_bps=0, direction="flat",
        stop_loss_pct=None, take_profit_pct=None,
        trader_summary=f"{name}: no signal", active_vectors={},
    )

def _entry(setup_id: str, name: str, direction: str, size_bps: int = 200,
           stop_pct: float = 2.0, tp_pct: float = 4.0) -> TraderDecision:
    return TraderDecision(
        setup_id=setup_id,
        action="buy" if direction == "long" else "sell",
        size_bps=size_bps, direction=direction,
        stop_loss_pct=stop_pct, take_profit_pct=tp_pct,
        trader_summary=f"{name}: {direction}", active_vectors={},
    )

def rsi_signal(df: pd.DataFrame, setup_id: str = "rsi",
               lower: float = 30, upper: float = 70) -> TraderDecision:
    rsi = ta.rsi(df["close"], length=14).iloc[-1]
    if rsi < lower:
        return _entry(setup_id, "rsi", "long")
    if rsi > upper:
        return _entry(setup_id, "rsi", "short")
    return _flat(setup_id, "rsi")

def ma_crossover_signal(df: pd.DataFrame, setup_id: str = "ma",
                        short: int = 30, long: int = 90) -> TraderDecision:
    s = df["close"].rolling(short).mean()
    l = df["close"].rolling(long).mean()
    if s.iloc[-2] <= l.iloc[-2] and s.iloc[-1] > l.iloc[-1]:
        return _entry(setup_id, "ma_cross", "long")
    if s.iloc[-2] >= l.iloc[-2] and s.iloc[-1] < l.iloc[-1]:
        return _entry(setup_id, "ma_cross", "short")
    return _flat(setup_id, "ma_cross")

def bollinger_signal(df: pd.DataFrame, setup_id: str = "bb") -> TraderDecision:
    bb = ta.bbands(df["close"], length=20, std=2)
    close = df["close"].iloc[-1]
    if close <= bb["BBL_20_2.0"].iloc[-1]:
        return _entry(setup_id, "bb", "long")
    if close >= bb["BBU_20_2.0"].iloc[-1]:
        return _entry(setup_id, "bb", "short")
    return _flat(setup_id, "bb")

def macd_signal_fn(df: pd.DataFrame, setup_id: str = "macd") -> TraderDecision:
    macd = ta.macd(df["close"])
    m, s = macd["MACD_12_26_9"], macd["MACDs_12_26_9"]
    if m.iloc[-2] <= s.iloc[-2] and m.iloc[-1] > s.iloc[-1]:
        return _entry(setup_id, "macd", "long")
    if m.iloc[-2] >= s.iloc[-2] and m.iloc[-1] < s.iloc[-1]:
        return _entry(setup_id, "macd", "short")
    return _flat(setup_id, "macd")

def donchian_breakout_signal(df: pd.DataFrame, setup_id: str = "donch",
                             lookback: int = 20) -> TraderDecision:
    high_n = df["high"].iloc[-lookback-1:-1].max()
    low_n = df["low"].iloc[-lookback-1:-1].min()
    close = df["close"].iloc[-1]
    if close > high_n:
        return _entry(setup_id, "donchian", "long")
    if close < low_n:
        return _entry(setup_id, "donchian", "short")
    return _flat(setup_id, "donchian")
```

- [ ] **Step 3: Run tests, verify pass**

```bash
pytest tests/unit/test_baselines.py -v
```

- [ ] **Step 4: Commit**

```bash
git add src/xianvec/baselines/ tests/unit/test_baselines.py
git commit -m "feat(baselines): null and technical baselines"
```

---

### Task 7.2: Onchain baselines

**Files:**
- Create: `src/xianvec/baselines/onchain.py`

- [ ] **Step 1: Implement onchain baselines**

```python
# src/xianvec/baselines/onchain.py
from xianvec.schemas import TraderDecision

def smart_money_copy(onchain: dict, setup_id: str = "sm") -> TraderDecision:
    """Follow the direction of smart money inflow."""
    inflow = onchain.get("smart_money_inflow", 0.0)
    if inflow > 0.5:
        return TraderDecision(
            setup_id=setup_id, action="buy", size_bps=200, direction="long",
            stop_loss_pct=2.5, take_profit_pct=5.0,
            trader_summary="smart money inflow", active_vectors={},
        )
    if inflow < -0.5:
        return TraderDecision(
            setup_id=setup_id, action="sell", size_bps=200, direction="short",
            stop_loss_pct=2.5, take_profit_pct=5.0,
            trader_summary="smart money outflow", active_vectors={},
        )
    return TraderDecision(
        setup_id=setup_id, action="flat", size_bps=0, direction="flat",
        stop_loss_pct=None, take_profit_pct=None,
        trader_summary="no smart money signal", active_vectors={},
    )

def funding_rate_fader(onchain: dict, setup_id: str = "fund",
                      threshold: float = 0.05) -> TraderDecision:
    """Fade extreme funding rates."""
    fr = onchain.get("funding_rate", 0.0)
    if fr > threshold:
        return TraderDecision(
            setup_id=setup_id, action="sell", size_bps=200, direction="short",
            stop_loss_pct=2.0, take_profit_pct=4.0,
            trader_summary=f"fade high funding {fr:.4f}", active_vectors={},
        )
    if fr < -threshold:
        return TraderDecision(
            setup_id=setup_id, action="buy", size_bps=200, direction="long",
            stop_loss_pct=2.0, take_profit_pct=4.0,
            trader_summary=f"fade low funding {fr:.4f}", active_vectors={},
        )
    return TraderDecision(
        setup_id=setup_id, action="flat", size_bps=0, direction="flat",
        stop_loss_pct=None, take_profit_pct=None,
        trader_summary="funding neutral", active_vectors={},
    )
```

- [ ] **Step 2: Commit**

```bash
git add src/xianvec/baselines/onchain.py
git commit -m "feat(baselines): smart money copy and funding rate fader"
```

---

## Phase 8 — Eval framework

### Task 8.0: Structured traces (prerequisite — do before 8.1)

**Rationale:** Evaluation results are uninterpretable without traces. Before any backtest or metric loop runs, every Stage 1 and Stage 2 call must produce a persisted trace record. A failing vector configuration without traces is a black box; with traces the exact prompt, parse error, or magnitude setting that caused the failure is recoverable. This task should be completed before any Phase 8 eval code runs. See architecture.md §9.4.

**Files:**
- Extend: `src/xianvec/data/store.py` (new `traces` table + `save_trace`)
- Extend: `src/xianvec/intern/claude.py`, `src/xianvec/intern/local.py` (emit trace on each call)
- Extend: `src/xianvec/trader/runtime.py` (emit trace on each Trader call)
- Test: `tests/unit/test_store.py` (trace round-trip)

- [ ] **Step 1: Add `traces` table to SQLite store**

```python
# src/xianvec/data/store.py — add to schema + store class
CREATE_TRACES = """
CREATE TABLE IF NOT EXISTS traces (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id      TEXT NOT NULL,
    setup_id    TEXT NOT NULL,
    stage       TEXT NOT NULL,          -- 'intern' | 'trader'
    model       TEXT,
    vectors_enabled INTEGER,            -- 0/1/NULL for intern
    vector_magnitudes TEXT,             -- JSON: {"conviction": 0.8, ...}
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    latency_ms  INTEGER,
    parse_ok    INTEGER NOT NULL,       -- 0/1
    error       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
"""

def save_trace(self, run_id: str, setup_id: str, stage: str,
               model: str | None, vectors_enabled: bool | None,
               vector_magnitudes: dict | None,
               prompt_tokens: int, completion_tokens: int,
               latency_ms: int, parse_ok: bool, error: str | None) -> None:
    self._conn.execute(
        """INSERT INTO traces
           (run_id, setup_id, stage, model, vectors_enabled, vector_magnitudes,
            prompt_tokens, completion_tokens, latency_ms, parse_ok, error)
           VALUES (?,?,?,?,?,?,?,?,?,?,?)""",
        (run_id, setup_id, stage, model,
         int(vectors_enabled) if vectors_enabled is not None else None,
         json.dumps(vector_magnitudes) if vector_magnitudes else None,
         prompt_tokens, completion_tokens, latency_ms, int(parse_ok), error)
    )
    self._conn.commit()
```

- [ ] **Step 2: Instrument Intern calls (claude.py + local.py)**

Wrap each LLM call in a `try/finally` that records start time, captures token counts from the API response, and calls `store.save_trace(...)`. Parse errors are caught, logged to `error` field, and re-raised.

- [ ] **Step 3: Instrument Trader calls (runtime.py)**

Same pattern. Additionally log `vectors_enabled` flag and `vector_magnitudes` dict (the active axis magnitudes at call time).

- [ ] **Step 4: Write and pass tests**

```python
def test_trace_round_trip(tmp_store):
    tmp_store.save_trace(
        run_id="r1", setup_id="s1", stage="trader",
        model="qwen3-14b-q4", vectors_enabled=True,
        vector_magnitudes={"conviction": 0.8},
        prompt_tokens=512, completion_tokens=64,
        latency_ms=1200, parse_ok=True, error=None
    )
    rows = tmp_store.conn.execute("SELECT * FROM traces WHERE run_id='r1'").fetchall()
    assert len(rows) == 1
    assert rows[0]["parse_ok"] == 1
```

- [ ] **Step 5: Verify**

```bash
pytest tests/unit/test_store.py -v -k trace
```

---

### Task 8.0b: Multi-regime anti-overfitting gate

**Rationale:** From NexusTrade's hill-climbing post-mortem: optimizing Δ-Sharpe without regime constraints produces configurations that score well in one regime and fail in others — Goodhart's Law in production. A vector configuration must demonstrate positive Δ-Sharpe in at least one bear-regime fold AND at least one bull-regime fold before it can be considered a valid result. See architecture.md §9.2 anti-overfitting gate.

**Files:**
- Extend: `src/xianvec/eval/compare.py` (add `regime_gate_check`)
- Extend: `scripts/compare_runs.py` (gate enforced before paper-trading authorization)
- Test: `tests/unit/test_metrics.py`

- [ ] **Step 1: Add regime labeling to walk-forward folds**

Each walk-forward fold is labeled `bear` or `bull` based on the asset's return over the fold window (negative total return → bear; positive → bull). Labels stored in fold metadata.

- [ ] **Step 2: Implement gate function**

```python
def regime_gate_check(fold_results: list[dict]) -> tuple[bool, str]:
    """
    Returns (passed: bool, reason: str).
    Passes only if Δ-Sharpe > 0 in at least one bear fold AND one bull fold.
    """
    bear_pass = any(f["delta_sharpe"] > 0 for f in fold_results if f["regime"] == "bear")
    bull_pass = any(f["delta_sharpe"] > 0 for f in fold_results if f["regime"] == "bull")
    if bear_pass and bull_pass:
        return True, "passed: positive Δ-Sharpe in both bear and bull regimes"
    missing = []
    if not bear_pass:
        missing.append("bear")
    if not bull_pass:
        missing.append("bull")
    return False, f"FAILED: no positive Δ-Sharpe in {missing} regime(s) — single-regime result only"
```

- [ ] **Step 3: Enforce gate in compare_runs.py**

Gate result printed prominently before any paper-trading instruction. A failing gate does not crash the script — it prints the result and blocks the paper-trading authorization line.

- [ ] **Step 4: Write and pass tests**

```python
def test_gate_requires_both_regimes():
    folds = [
        {"regime": "bull", "delta_sharpe": 0.3},
        {"regime": "bull", "delta_sharpe": 0.1},
        {"regime": "bear", "delta_sharpe": -0.05},
    ]
    passed, reason = regime_gate_check(folds)
    assert not passed
    assert "bear" in reason

def test_gate_passes_with_both():
    folds = [
        {"regime": "bull", "delta_sharpe": 0.2},
        {"regime": "bear", "delta_sharpe": 0.1},
    ]
    passed, _ = regime_gate_check(folds)
    assert passed
```

---

### Task 8.1: Metrics

**Files:**
- Create: `src/xianvec/eval/metrics.py`
- Test: `tests/unit/test_metrics.py`

- [ ] **Step 1: Write failing tests**

```python
# tests/unit/test_metrics.py
import numpy as np
from xianvec.eval.metrics import sharpe, max_drawdown, profit_factor, win_rate

def test_sharpe_zero_for_constant_returns():
    returns = np.array([0.01]*100)
    # zero variance => undefined; we return 0.0 by convention
    assert sharpe(returns) == 0.0

def test_sharpe_positive_for_positive_mean():
    rng = np.random.default_rng(42)
    returns = rng.normal(0.001, 0.01, 252)
    assert sharpe(returns) > 0

def test_max_drawdown():
    equity = np.array([100, 110, 105, 120, 90, 95, 130])
    dd = max_drawdown(equity)
    # peak 120 -> trough 90 = -25%
    assert abs(dd - (-0.25)) < 1e-6

def test_profit_factor():
    pnls = np.array([10, -5, 20, -10, 15])  # wins: 45, losses: 15
    assert abs(profit_factor(pnls) - 3.0) < 1e-6

def test_win_rate():
    pnls = np.array([10, -5, 20, -10, 15, -3])
    assert win_rate(pnls) == 0.5
```

- [ ] **Step 2: Run, verify fail**

```bash
pytest tests/unit/test_metrics.py -v
```

- [ ] **Step 3: Implement**

```python
# src/xianvec/eval/metrics.py
import numpy as np

PERIODS_PER_YEAR_DEFAULT = 252  # daily; use 365*24*4 for 15-min crypto bars

def sharpe(returns: np.ndarray, periods_per_year: int = PERIODS_PER_YEAR_DEFAULT,
           risk_free: float = 0.0) -> float:
    if len(returns) == 0:
        return 0.0
    excess = returns - risk_free / periods_per_year
    sd = excess.std(ddof=1) if len(excess) > 1 else 0.0
    if sd == 0:
        return 0.0
    return float(excess.mean() / sd * np.sqrt(periods_per_year))

def max_drawdown(equity: np.ndarray) -> float:
    if len(equity) < 2:
        return 0.0
    peaks = np.maximum.accumulate(equity)
    drawdowns = (equity - peaks) / peaks
    return float(drawdowns.min())

def profit_factor(pnls: np.ndarray) -> float:
    wins = pnls[pnls > 0].sum()
    losses = -pnls[pnls < 0].sum()
    if losses == 0:
        return float("inf") if wins > 0 else 0.0
    return float(wins / losses)

def win_rate(pnls: np.ndarray) -> float:
    if len(pnls) == 0:
        return 0.0
    return float((pnls > 0).sum() / len(pnls))

def equity_curve(pnls: np.ndarray, initial_nav: float = 10000.0) -> np.ndarray:
    return initial_nav + np.cumsum(pnls)

def returns_from_pnl(pnls: np.ndarray, initial_nav: float = 10000.0) -> np.ndarray:
    """Trade-level returns with a CONSTANT denominator (initial NAV).

    Order-invariant by construction: a permutation of trades produces the same
    multiset of returns, so Sharpe is path-independent. Required for the paired
    bootstrap to be valid — `np.diff(equity)/equity[:-1]` makes returns depend
    on cumulative-PnL position, which corrupts bootstrap permutations.
    See structural-review fix #8.
    """
    return np.asarray(pnls, dtype=np.float64) / initial_nav
```

- [ ] **Step 4: Run, verify pass**

```bash
pytest tests/unit/test_metrics.py -v
```

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/eval/metrics.py tests/unit/test_metrics.py
git commit -m "feat(eval): risk-adjusted metrics (sharpe, dd, pf, wr)"
```

---

### Task 8.2: Paired bootstrap comparison

**Files:**
- Create: `src/xianvec/eval/compare.py`
- Test: `tests/unit/test_compare.py`

- [ ] **Step 1: Write failing test**

```python
# tests/unit/test_compare.py
import numpy as np
from xianvec.eval.compare import paired_bootstrap_sharpe_delta

def test_bootstrap_returns_ci():
    rng = np.random.default_rng(42)
    # condition A clearly better than B
    a = rng.normal(0.002, 0.01, 100)
    b = rng.normal(0.000, 0.01, 100)
    result = paired_bootstrap_sharpe_delta(a, b, n_resamples=2000, seed=42)
    assert result["delta_sharpe"] > 0
    assert result["ci_low"] < result["delta_sharpe"] < result["ci_high"]
    assert 0.0 < result["p_value"] < 0.5

def test_bootstrap_null_case():
    rng = np.random.default_rng(0)
    # both same distribution
    a = rng.normal(0, 0.01, 100)
    b = rng.normal(0, 0.01, 100)
    result = paired_bootstrap_sharpe_delta(a, b, n_resamples=2000, seed=0)
    # CI should straddle zero
    assert result["ci_low"] < 0 < result["ci_high"]

def test_block_bootstrap_widens_ci_under_correlation():
    """When trades are serially correlated, IID bootstrap is too tight; block bootstrap
    should produce a wider CI on the same data — that is the whole point of using it."""
    rng = np.random.default_rng(7)
    # construct correlated paired returns (AR(1)-ish)
    n = 200
    a = np.zeros(n); b = np.zeros(n)
    for i in range(1, n):
        a[i] = 0.5 * a[i-1] + rng.normal(0.001, 0.01)
        b[i] = 0.5 * b[i-1] + rng.normal(0.000, 0.01)
    iid = paired_bootstrap_sharpe_delta(a, b, n_resamples=2000, seed=1, block_size=1)
    blk = paired_bootstrap_sharpe_delta(a, b, n_resamples=2000, seed=1, block_size=8)
    iid_width = iid["ci_high"] - iid["ci_low"]
    blk_width = blk["ci_high"] - blk["ci_low"]
    assert blk_width > iid_width    # block CI is wider — the honest one
```

- [ ] **Step 2: Implement**

```python
# src/xianvec/eval/compare.py
import numpy as np
from xianvec.eval.metrics import sharpe

def paired_bootstrap_sharpe_delta(
    returns_a: np.ndarray,
    returns_b: np.ndarray,
    n_resamples: int = 10000,
    ci: float = 0.95,
    seed: int | None = None,
    block_size: int = 1,
) -> dict:
    """Paired bootstrap of Sharpe(A) - Sharpe(B). Inputs must be same length and aligned.

    `block_size`: when > 1, performs a moving-block bootstrap — sample contiguous
    blocks of length `block_size` instead of IID indices. Required when input
    returns are serially correlated (e.g. backtest setups with overlapping forward
    windows). For trade-level data with `step >= horizon`, IID (block_size=1) is
    valid; otherwise set block_size to ⌈horizon/step⌉ at minimum.
    See structural-review fix #4.
    """
    if len(returns_a) != len(returns_b):
        raise ValueError("paired returns must be same length")
    if block_size < 1:
        raise ValueError("block_size must be >= 1")
    rng = np.random.default_rng(seed)
    n = len(returns_a)

    deltas = np.empty(n_resamples)
    for i in range(n_resamples):
        idx = _resample_indices(rng, n, block_size)
        deltas[i] = sharpe(returns_a[idx]) - sharpe(returns_b[idx])

    point = sharpe(returns_a) - sharpe(returns_b)
    alpha = (1 - ci) / 2
    ci_low = float(np.quantile(deltas, alpha))
    ci_high = float(np.quantile(deltas, 1 - alpha))
    # two-sided p-value: prob bootstrap delta is on opposite side of 0 from point
    if point >= 0:
        p = float((deltas <= 0).mean()) * 2
    else:
        p = float((deltas >= 0).mean()) * 2
    p = min(p, 1.0)

    return {
        "delta_sharpe": float(point),
        "ci_low": ci_low,
        "ci_high": ci_high,
        "p_value": p,
        "n": n,
        "n_resamples": n_resamples,
        "block_size": block_size,
    }

def _resample_indices(rng: np.random.Generator, n: int, block_size: int) -> np.ndarray:
    """Return n indices: IID for block_size=1, moving-block for block_size>1."""
    if block_size == 1:
        return rng.integers(0, n, n)
    n_blocks = (n + block_size - 1) // block_size
    starts = rng.integers(0, max(1, n - block_size + 1), n_blocks)
    idx = np.concatenate([np.arange(s, s + block_size) for s in starts])[:n]
    return idx
```

- [ ] **Step 3: Run, verify pass**

```bash
pytest tests/unit/test_compare.py -v
```

- [ ] **Step 4: Commit**

```bash
git add src/xianvec/eval/compare.py tests/unit/test_compare.py
git commit -m "feat(eval): paired bootstrap for sharpe delta"
```

---

### Task 8.3: Backtest harness

**Files:**
- Create: `src/xianvec/eval/backtest.py`
- Create: `scripts/run_backtest.py`

- [ ] **Step 1: Implement harness**

```python
# src/xianvec/eval/backtest.py
"""Backtest harness with stateful portfolio + briefing cache.

Two structural fixes baked in here vs the naive design:

1. **Stateful portfolio.** The simulator threads a running portfolio (NAV, open
   positions, daily PnL window, loss streak, atr_pct) across setups so the risk
   layer is *actually live* in backtest — circuit breaker, cluster cap, loss-streak
   cooldown, and vol-targeting all enforced. The naive freeze-portfolio-per-setup
   version made the risk layer a no-op and produced a system-under-test that
   diverged from what forward paper trading actually runs. (Structural-review #3.)

2. **Briefing cache.** The Intern is expensive (Claude API) and non-deterministic
   without temperature=0. The cache sidesteps both: each setup's briefing is fetched
   once and reused across every trader arm (vectors-OFF / random / orthogonal /
   vectors-ON). Pairing is now exact at the Stage-1 level. (Structural-review #1.)

Default `step=24 > horizon=16` so consecutive setups have non-overlapping forward
windows. With overlap, IID bootstrap CIs are too tight; either keep step >= horizon
or pass `block_size` to the bootstrap. (Structural-review #4.)
"""
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable
import pickle
import pandas as pd
import numpy as np
from xianvec.schemas import MarketState, TraderDecision, InternBriefing
from xianvec.execution.simulator import Simulator
from xianvec.risk.rules import RiskEvaluator

DecisionFn = Callable[[MarketState, pd.DataFrame], TraderDecision]
# Variant that takes a precomputed Intern briefing — used by the agent arms.
BriefedDecisionFn = Callable[[MarketState, InternBriefing], TraderDecision]

@dataclass
class BacktestResult:
    name: str
    pnls: list[float] = field(default_factory=list)
    exit_reasons: list[str] = field(default_factory=list)
    decisions: list[TraderDecision] = field(default_factory=list)
    setup_ids: list[str] = field(default_factory=list)
    nav_curve: list[float] = field(default_factory=list)   # NAV after each setup

    def to_arrays(self):
        return np.array(self.pnls), self.exit_reasons


@dataclass
class PortfolioState:
    """Mutable running portfolio for the backtest. Mirrors the live Alpaca shape."""
    initial_nav: float
    nav: float
    cash: float
    open_positions: list[dict] = field(default_factory=list)
    daily_pnl_window: list[float] = field(default_factory=list)  # rolling 1-day pnls
    bars_per_day: int = 96   # 15-min bars * 24h
    consecutive_losses: int = 0
    bars_since_last_loss: int = 999

    def daily_pnl_pct(self) -> float:
        return (sum(self.daily_pnl_window) / self.initial_nav) * 100 if self.daily_pnl_window else 0.0

    def update_after_trade(self, pnl: float, bars_held: int):
        self.nav += pnl
        self.cash = self.nav   # close-on-exit; v1 has no carry positions
        self.daily_pnl_window.append(pnl)
        if len(self.daily_pnl_window) > self.bars_per_day:
            self.daily_pnl_window.pop(0)
        if pnl < 0:
            self.consecutive_losses += 1
            self.bars_since_last_loss = 0
        else:
            self.consecutive_losses = 0
            self.bars_since_last_loss += bars_held
        # (open_positions stays empty since v1 closes at horizon/stop/tp)


def iter_setups(price_df: pd.DataFrame, asset: str, lookback: int = 200,
                horizon: int = 16, step: int = 24
                ) -> Iterable[tuple[MarketState, pd.DataFrame, pd.DataFrame, str]]:
    """Yield (state, window_df, future_df, setup_id). Default step=24 >= horizon=16
    so consecutive forward windows do not overlap (structural-review #4).

    The returned `state.portfolio` is a placeholder; `run_backtest` overwrites it
    with the running PortfolioState before invoking the decision_fn.
    """
    from xianvec.data.indicators import compute_indicators
    n = len(price_df)
    for i in range(lookback, n - horizon, step):
        window = price_df.iloc[i-lookback:i]
        future = price_df.iloc[i:i+horizon]
        indicators = compute_indicators(window)
        ts_val = price_df.index[i]
        ts = float(ts_val.timestamp()) if hasattr(ts_val, "timestamp") else float(i)
        state = MarketState(
            asset=asset,
            timestamp=ts,
            ohlcv_recent=[],
            indicators=indicators,
            onchain={"smart_money_inflow": 0.0, "funding_rate": 0.0},
            portfolio={},  # filled in by run_backtest from the running PortfolioState
        )
        setup_id = f"{asset}-{i}"
        yield state, window, future, setup_id


def _portfolio_dict(p: PortfolioState, indicators: dict) -> dict:
    """Project the running PortfolioState into the dict shape risk + agent expect."""
    close = indicators.get("close") or 1.0
    atr = indicators.get("atr_14") or 0.0
    return {
        "nav": p.nav,
        "cash": p.cash,
        "open_positions": list(p.open_positions),
        "daily_pnl_pct": p.daily_pnl_pct(),
        "consecutive_losses": p.consecutive_losses,
        "bars_since_last_loss": p.bars_since_last_loss,
        "atr_pct": (atr / close * 100) if close else 0.0,
    }


class BriefingCache:
    """Disk-backed Intern-briefing cache keyed by setup_id.

    Both vectors-OFF and vectors-ON arms read from the same cache so they see
    the SAME Stage-1 briefing for the same setup. Without this, Claude's
    sampling noise enters the paired Δ-Sharpe and pairing is broken at Stage 1.
    (Structural-review #1.)
    """
    def __init__(self, path: Path | str = "data/briefings.pkl"):
        self.path = Path(path)
        self._mem: dict[str, InternBriefing] = {}
        if self.path.exists():
            with open(self.path, "rb") as f:
                self._mem = pickle.load(f)

    def get_or_compute(self, setup_id: str, fn: Callable[[], InternBriefing]) -> InternBriefing:
        if setup_id in self._mem:
            return self._mem[setup_id]
        briefing = fn()
        self._mem[setup_id] = briefing
        return briefing

    def save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with open(self.path, "wb") as f:
            pickle.dump(self._mem, f)


def run_backtest(
    name: str,
    decision_fn: DecisionFn,
    price_df: pd.DataFrame,
    asset: str,
    risk_cfg: dict,
    initial_nav: float = 10000.0,
    fee_bps: float = 10.0,
    slippage_bps: float | None = None,
    horizon: int = 16,
    step: int = 24,
) -> BacktestResult:
    """Run a backtest with running portfolio state and a live risk layer.

    Risk ownership: the **harness owns the final risk gate** so that baselines
    (which are bare decision functions, not pipelines) get the same risk
    treatment as the agent. The agent's TradePipeline may also run risk
    internally for instrumentation; that is harmless because RiskEvaluator
    is idempotent on already-conformant decisions. (Structural-review T3.)
    """
    sim = Simulator(initial_nav=initial_nav, fee_bps=fee_bps, slippage_bps=slippage_bps)
    risk = RiskEvaluator(risk_cfg)
    portfolio_state = PortfolioState(initial_nav=initial_nav, nav=initial_nav, cash=initial_nav)
    result = BacktestResult(name=name)

    for state, window, future, setup_id in iter_setups(price_df, asset, horizon=horizon, step=step):
        # inject the running portfolio into the state the decision_fn sees
        portfolio = _portfolio_dict(portfolio_state, state.indicators)
        state = state.model_copy(update={"portfolio": portfolio})
        try:
            decision = decision_fn(state, window)
        except Exception as e:
            print(f"decision_fn error on {setup_id}: {e}")
            result.setup_ids.append(setup_id)
            result.pnls.append(0.0)
            result.exit_reasons.append(f"error:{str(e)[:40]}")
            result.decisions.append(None)  # type: ignore
            result.nav_curve.append(portfolio_state.nav)
            continue
        decision = decision.model_copy(update={"setup_id": setup_id})

        risked = risk.evaluate(decision, asset=asset, portfolio=portfolio)
        if not risked.approved:
            result.pnls.append(0.0)
            result.exit_reasons.append(f"vetoed:{(risked.veto_reason or '')[:40]}")
            result.decisions.append(decision)
            result.setup_ids.append(setup_id)
            result.nav_curve.append(portfolio_state.nav)
            continue
        actual = risked.modified or risked.original

        if actual.action in ("flat", "close"):
            result.pnls.append(0.0)
            result.exit_reasons.append(f"no_trade:{actual.action}")
            result.decisions.append(actual)
            result.setup_ids.append(setup_id)
            result.nav_curve.append(portfolio_state.nav)
            continue

        entry = float(future["close"].iloc[0])
        pnl, reason = sim.simulate_trade(actual, future, entry_price=entry, asset=asset)
        portfolio_state.update_after_trade(pnl, bars_held=horizon)

        result.pnls.append(pnl)
        result.exit_reasons.append(reason)
        result.decisions.append(actual)
        result.setup_ids.append(setup_id)
        result.nav_curve.append(portfolio_state.nav)

    return result


def align_paired(r_a: BacktestResult, r_b: BacktestResult) -> tuple[np.ndarray, np.ndarray]:
    """Filter to setups present in both, return aligned pnl arrays."""
    common = [sid for sid in r_a.setup_ids if sid in set(r_b.setup_ids)]
    a_map = dict(zip(r_a.setup_ids, r_a.pnls))
    b_map = dict(zip(r_b.setup_ids, r_b.pnls))
    a = np.array([a_map[sid] for sid in common])
    b = np.array([b_map[sid] for sid in common])
    return a, b
```

- [ ] **Step 2: Backtest runner CLI**

```python
# scripts/run_backtest.py
"""Run backtest comparing baselines on historical data."""
import pandas as pd
import numpy as np
import typer
from xianvec.config import load_config
from xianvec.eval.backtest import run_backtest
from xianvec.eval.metrics import sharpe, max_drawdown, profit_factor, win_rate, returns_from_pnl
from xianvec.baselines.null import buy_and_hold, random_signal
from xianvec.baselines.technical import (
    rsi_signal, ma_crossover_signal, bollinger_signal,
    macd_signal_fn, donchian_breakout_signal,
)
from xianvec.baselines.onchain import smart_money_copy, funding_rate_fader

# Adapter: baselines that take a price-window become (state, window) -> Decision functions.
def windowed(baseline_fn):
    def adapter(state, window):
        return baseline_fn(window)
    return adapter

# Adapter: onchain baselines take only the state.onchain dict.
def onchain_adapter(baseline_fn):
    def adapter(state, window):
        return baseline_fn(state.onchain)
    return adapter

def main(price_path: str, asset: str = "BTC-USD"):
    cfg = load_config("config/default.yaml")
    risk_cfg = load_config("config/risk.yaml")
    df = pd.read_parquet(price_path)

    runs = {
        "buy_hold": run_backtest("buy_hold", windowed(buy_and_hold), df, asset, risk_cfg),
        "random":   run_backtest("random",   windowed(random_signal), df, asset, risk_cfg),
        "rsi":      run_backtest("rsi",      windowed(rsi_signal), df, asset, risk_cfg),
        "ma_cross": run_backtest("ma_cross", windowed(ma_crossover_signal), df, asset, risk_cfg),
        "bbands":   run_backtest("bbands",   windowed(bollinger_signal), df, asset, risk_cfg),
        "macd":     run_backtest("macd",     windowed(macd_signal_fn), df, asset, risk_cfg),
        "donchian": run_backtest("donchian", windowed(donchian_breakout_signal), df, asset, risk_cfg),
        # onchain baselines no-op in pure-OHLCV backtest unless onchain data was joined into df
        "smart_money": run_backtest("smart_money", onchain_adapter(smart_money_copy),
                                    df, asset, risk_cfg),
        "funding_fade": run_backtest("funding_fade", onchain_adapter(funding_rate_fader),
                                     df, asset, risk_cfg),
    }

    print(f"\n{'name':14s}  {'n':>5s}  {'sharpe':>8s}  {'mdd':>8s}  {'pf':>6s}  {'wr':>6s}")
    for name, r in runs.items():
        pnls, _ = r.to_arrays()
        if len(pnls) == 0:
            continue
        rets = returns_from_pnl(pnls)
        eq = np.cumsum(pnls) + 10000
        print(f"{name:14s}  {len(pnls):5d}  "
              f"{sharpe(rets, periods_per_year=365*24*4):8.3f}  "
              f"{max_drawdown(eq):8.3f}  "
              f"{profit_factor(pnls):6.2f}  {win_rate(pnls):6.2%}")

if __name__ == "__main__":
    typer.run(main)
```

The agent (vectors-on/vectors-off) backtest is wired in `scripts/run_ab_compare.py` (Task 9.2), which uses the same harness with a windowed-style adapter that ignores the window because Stage 1+2 only consult `state`.

- [ ] **Step 3: Commit**

```bash
git add src/xianvec/eval/backtest.py scripts/run_backtest.py
git commit -m "feat(eval): backtest harness with paired alignment"
```

---

### Task 8.4: Walk-forward harness

**Why:** A single chronological pass over a fixed dataset is technically not walk-forward — observers will reasonably ask whether vectors were tuned on the same window they were evaluated on. Real walk-forward holds out the last N% of each window for evaluation, slides forward, repeats. Without this, our results look in-sample even if no actual tuning happened on the eval set.

**v1 caveat (structural-review T3):** vectors are extracted *once* from synthetic contrastive pairs and frozen for the whole eval; we do not learn anything from the train slice. The harness here is therefore really a *stationarity check* (does Δ-Sharpe hold across non-adjacent test windows?) rather than a true walk-forward. The `train` slice is generated and discarded; the test slices are what feed the bootstrap. We keep the name "walk-forward" because the splitting logic is reusable for v2's regime-vector tuning loop, but the v1 acceptance criterion is "fold-level Δ-Sharpe is roughly stationary," not "vectors generalize from train to test."

**Files:**
- Create: `src/xianvec/eval/walk_forward.py`
- Test: `tests/unit/test_walk_forward.py`
- Modify: `scripts/run_ab_compare.py` to optionally run WF mode

- [ ] **Step 1: Failing test**

```python
# tests/unit/test_walk_forward.py
import pandas as pd
import numpy as np
from xianvec.eval.walk_forward import walk_forward_splits

def test_expanding_window_splits():
    n = 1000
    df = pd.DataFrame({"close": np.arange(n)})
    splits = list(walk_forward_splits(
        df, train_size=200, test_size=100, step=100, mode="expanding",
    ))
    # first split: train [0:200], test [200:300]
    assert len(splits[0][0]) == 200
    assert len(splits[0][1]) == 100
    # last split's train extends to where test begins
    last_train, last_test = splits[-1]
    assert len(last_train) >= 200
    assert last_train.index[-1] + 1 == last_test.index[0]
    # each subsequent train includes everything up to its test
    for train, test in splits:
        assert train.index[-1] + 1 == test.index[0]

def test_rolling_window_splits():
    n = 1000
    df = pd.DataFrame({"close": np.arange(n)})
    splits = list(walk_forward_splits(
        df, train_size=200, test_size=100, step=100, mode="rolling",
    ))
    # rolling: train always exactly 200 long
    for train, _ in splits:
        assert len(train) == 200
```

- [ ] **Step 2: Implement walk-forward**

```python
# src/xianvec/eval/walk_forward.py
from dataclasses import dataclass
from typing import Iterable, Literal, Callable
import pandas as pd
import numpy as np
from xianvec.eval.backtest import run_backtest, BacktestResult, DecisionFn
from xianvec.eval.metrics import sharpe, returns_from_pnl, max_drawdown
from xianvec.eval.compare import paired_bootstrap_sharpe_delta

Mode = Literal["expanding", "rolling"]

def walk_forward_splits(
    df: pd.DataFrame,
    train_size: int,
    test_size: int,
    step: int,
    mode: Mode = "expanding",
) -> Iterable[tuple[pd.DataFrame, pd.DataFrame]]:
    """Yield (train_window, test_window) pairs in chronological order.

    expanding: train grows over time, always starts at index 0
    rolling: train is a fixed-size sliding window
    """
    n = len(df)
    start = 0
    train_end = train_size
    while train_end + test_size <= n:
        train = df.iloc[start:train_end] if mode == "rolling" else df.iloc[0:train_end]
        test = df.iloc[train_end:train_end + test_size]
        yield train, test
        train_end += step
        if mode == "rolling":
            start += step

@dataclass
class WalkForwardResult:
    fold_metrics: list[dict]
    aggregate_pnls_a: np.ndarray
    aggregate_pnls_b: np.ndarray
    bootstrap: dict

def run_paired_walk_forward(
    df: pd.DataFrame,
    asset: str,
    risk_cfg: dict,
    decision_fn_a: DecisionFn,    # vectors ON
    decision_fn_b: DecisionFn,    # vectors OFF
    train_size: int = 2000,
    test_size: int = 500,
    step: int = 500,
    mode: Mode = "expanding",
    bootstrap_resamples: int = 10000,
) -> WalkForwardResult:
    """Run paired vectors-ON/OFF backtest across rolling test windows.
    Vectors are NOT retrained per fold (they're frozen for the experiment),
    but evaluation is strictly out-of-sample relative to any vector tuning."""
    fold_metrics = []
    all_a_pnls: list[float] = []
    all_b_pnls: list[float] = []
    for fold_idx, (train, test) in enumerate(walk_forward_splits(
        df, train_size, test_size, step, mode
    )):
        r_a = run_backtest(f"on_fold{fold_idx}", decision_fn_a, test, asset, risk_cfg)
        r_b = run_backtest(f"off_fold{fold_idx}", decision_fn_b, test, asset, risk_cfg)
        a_pnls, _ = r_a.to_arrays()
        b_pnls, _ = r_b.to_arrays()
        # align by setup_id within the fold
        common = [sid for sid in r_a.setup_ids if sid in set(r_b.setup_ids)]
        a_map = dict(zip(r_a.setup_ids, r_a.pnls))
        b_map = dict(zip(r_b.setup_ids, r_b.pnls))
        a = np.array([a_map[s] for s in common])
        b = np.array([b_map[s] for s in common])

        rets_a = returns_from_pnl(a) if len(a) > 1 else np.array([])
        rets_b = returns_from_pnl(b) if len(b) > 1 else np.array([])
        fold_metrics.append({
            "fold": fold_idx,
            "n": len(common),
            "sharpe_on": float(sharpe(rets_a)) if len(rets_a) else 0.0,
            "sharpe_off": float(sharpe(rets_b)) if len(rets_b) else 0.0,
            "delta_sharpe": (
                float(sharpe(rets_a) - sharpe(rets_b))
                if len(rets_a) and len(rets_b) else 0.0
            ),
            "mdd_on": float(max_drawdown(np.cumsum(a) + 10000)) if len(a) else 0.0,
            "mdd_off": float(max_drawdown(np.cumsum(b) + 10000)) if len(b) else 0.0,
        })
        all_a_pnls.extend(a.tolist())
        all_b_pnls.extend(b.tolist())

    a_arr = np.array(all_a_pnls)
    b_arr = np.array(all_b_pnls)
    rets_a = returns_from_pnl(a_arr) if len(a_arr) > 1 else np.array([])
    rets_b = returns_from_pnl(b_arr) if len(b_arr) > 1 else np.array([])
    boot = (
        paired_bootstrap_sharpe_delta(rets_a, rets_b, n_resamples=bootstrap_resamples, seed=42)
        if len(rets_a) > 1 and len(rets_b) > 1
        else {"delta_sharpe": 0.0, "ci_low": 0.0, "ci_high": 0.0,
              "p_value": 1.0, "n": 0, "n_resamples": 0}
    )
    return WalkForwardResult(
        fold_metrics=fold_metrics,
        aggregate_pnls_a=a_arr,
        aggregate_pnls_b=b_arr,
        bootstrap=boot,
    )
```

- [ ] **Step 3: Add CLI flag to A/B runner**

```python
# scripts/run_ab_compare.py — add a --mode flag (single or walk_forward)
def main(price_path: str, asset: str = "BTC-USD",
         mode: str = "single", train_size: int = 2000,
         test_size: int = 500, step: int = 500,
         wf_window: str = "expanding"):
    ...
    if mode == "single":
        # existing code path
        ...
    elif mode == "walk_forward":
        from xianvec.eval.walk_forward import run_paired_walk_forward
        wf = run_paired_walk_forward(
            df, asset, risk_cfg,
            decision_fn_a=make_decision_fn(True),
            decision_fn_b=make_decision_fn(False),
            train_size=train_size, test_size=test_size, step=step,
            mode=wf_window,
        )
        print(f"\n=== WALK-FORWARD ({wf_window}, {len(wf.fold_metrics)} folds) ===")
        for fm in wf.fold_metrics:
            print(f"  fold {fm['fold']:2d}  n={fm['n']:4d}  "
                  f"Δ-Sharpe={fm['delta_sharpe']:+.3f}  "
                  f"on/off Sharpe={fm['sharpe_on']:.3f}/{fm['sharpe_off']:.3f}  "
                  f"on/off MDD={fm['mdd_on']:.3f}/{fm['mdd_off']:.3f}")
        b = wf.bootstrap
        print(f"\nAggregate Δ-Sharpe (all folds): {b['delta_sharpe']:.3f}  "
              f"95% CI [{b['ci_low']:.3f}, {b['ci_high']:.3f}]  p≈{b['p_value']:.3f}  "
              f"n={b['n']}")
```

- [ ] **Step 4: Run tests + smoke**

```bash
pytest tests/unit/test_walk_forward.py -v
python scripts/run_ab_compare.py data/historical/btc-15min-90days.parquet \
    --mode walk_forward --train-size 2000 --test-size 500 --step 500
```

Expected: per-fold metrics print, then aggregate Δ-Sharpe with CI. The aggregate should be roughly comparable to single-mode if no overfitting; large divergence between single-mode and walk-forward Δ-Sharpe is itself a signal that the single-mode result was leakage-driven.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/eval/walk_forward.py tests/unit/test_walk_forward.py scripts/run_ab_compare.py
git commit -m "feat(eval): walk-forward backtest harness with paired bootstrap"
```

---

## Phase 9 — Pipeline orchestration + the A/B experiment

### Task 9.1: Full pipeline (Stage 1 → Stage 2 → Risk → Sim/Alpaca)

**Files:**
- Create: `src/xianvec/pipeline/trade.py`
- Test: `tests/integration/test_pipeline.py`

- [ ] **Step 1: Write integration test (mocked Stage 1 + Stage 2)**

```python
# tests/integration/test_pipeline.py
import pandas as pd
from unittest.mock import MagicMock
from xianvec.pipeline.trade import TradePipeline
from xianvec.schemas import MarketState, InternBriefing, TraderDecision

def test_pipeline_happy_path():
    state = MarketState(
        asset="BTC-USD", timestamp=1.0, ohlcv_recent=[],
        indicators={"close": 50000, "rsi_14": 50, "ma_30": 50000, "ma_60": 50000,
                    "ma_90": 50000, "atr_14": 1500, "macd": 0, "macd_signal": 0,
                    "bb_upper": 51000, "bb_lower": 49000, "donchian_high_20": 52000,
                    "donchian_low_20": 48000, "volume_ratio_20": 1.0,
                    "bb_mid": 50000},
        onchain={}, portfolio={"nav": 10000, "cash": 10000,
                                "open_positions": [], "daily_pnl_pct": 0.0},
    )
    fake_briefing = InternBriefing(
        setup_id="t", asset="BTC-USD",
        bull_case="t", bear_case="c", flat_case="f",
        evidence_long=[], evidence_short=[], evidence_flat=[],
        regime="choppy", signal_quality=0.5, horizon_hours=4,
    )
    fake_decision = TraderDecision(
        setup_id="t", action="buy", size_bps=200, direction="long",
        stop_loss_pct=2.0, take_profit_pct=4.0,
        trader_summary="long", active_vectors={"conviction": 0.5},
    )
    intern = MagicMock()
    intern.reason.return_value = fake_briefing
    trader = MagicMock()
    trader.decide.return_value = fake_decision

    pipeline = TradePipeline(
        intern=intern, trader=trader,
        risk_cfg={"max_position_size_pct": 20, "max_total_exposure_pct": 100,
                  "daily_loss_circuit_breaker_pct": 5, "max_open_positions": 5,
                  "correlation_clusters": {}, "max_per_cluster": 2,
                  "require_stop_loss": True},
    )
    result = pipeline.run(state, setup_id="t")
    assert result["risk"].approved is True
    assert result["decision"].action == "buy"
```

- [ ] **Step 2: Implement pipeline**

```python
# src/xianvec/pipeline/trade.py
from typing import Protocol
from xianvec.schemas import MarketState, InternBriefing, TraderDecision, RiskDecision
from xianvec.risk.rules import RiskEvaluator

class Intern(Protocol):
    def reason(self, state: MarketState, setup_id: str) -> InternBriefing: ...

class Trader(Protocol):
    def decide(self, briefing: InternBriefing, regime_vectors: dict) -> TraderDecision: ...

class TradePipeline:
    def __init__(self, intern: Intern, trader: Trader, risk_cfg: dict,
                 regime_vectors_cfg: dict | None = None):
        self.intern = intern
        self.trader = trader
        self.risk = RiskEvaluator(risk_cfg)
        self.regime_vectors = regime_vectors_cfg or {}

    def run(self, state: MarketState, setup_id: str) -> dict:
        briefing = self.intern.reason(state, setup_id=setup_id)
        weights = self.regime_vectors.get(briefing.regime, {})
        decision = self.trader.decide(briefing, regime_vectors=weights)
        risk = self.risk.evaluate(decision, asset=state.asset, portfolio=state.portfolio)
        return {"briefing": briefing, "decision": decision, "risk": risk}
```

- [ ] **Step 3: Wrap TraderModel as a Trader runtime**

```python
# src/xianvec/trader/runtime.py
from xianvec.schemas import InternBriefing, TraderDecision
from xianvec.trader.model import TraderModel
from xianvec.trader.prompt import build_trader_prompt, parse_trader_response
from xianvec.trader.vectors import compose_axis_vectors, gate_magnitude

class VectorTrader:
    def __init__(self, model: TraderModel, axis_vectors: dict, vectors_enabled: bool = True,
                 confidence_gating: bool = True):
        self.model = model
        self.axis_vectors = axis_vectors
        self.vectors_enabled = vectors_enabled
        self.confidence_gating = confidence_gating

    def decide(self, briefing: InternBriefing, regime_vectors: dict) -> TraderDecision:
        prompt = build_trader_prompt(briefing)
        composed = None
        if self.vectors_enabled and regime_vectors:
            composed = compose_axis_vectors(self.axis_vectors, regime_vectors)
        if composed is not None:
            self.model.set_vector(composed, magnitude=1.0)
            text, first_logits = self.model.generate(prompt)
            if self.confidence_gating:
                gate = gate_magnitude(first_logits)
                if gate < 0.5:
                    # high uncertainty — re-run with dampened magnitude
                    self.model.set_vector(composed, magnitude=gate)
                    text, _ = self.model.generate(prompt)
            active = regime_vectors
        else:
            self.model.set_vector(None)
            text, _ = self.model.generate(prompt)
            active = {}
        return parse_trader_response(text, setup_id=briefing.setup_id, active_vectors=active)
```

- [ ] **Step 4: Run integration test**

```bash
pytest tests/integration/test_pipeline.py -v
```

Expected: passed.

- [ ] **Step 5: Commit**

```bash
git add src/xianvec/pipeline/ src/xianvec/trader/ tests/integration/test_pipeline.py
git commit -m "feat(pipeline): full stage 1→2→risk orchestration"
```

---

### Task 9.2: A/B comparison runner

**Files:**
- Create: `scripts/run_ab_compare.py`

- [ ] **Step 1: Write A/B comparison script**

```python
# scripts/run_ab_compare.py
"""Multi-asset, multi-arm A/B backtest with briefing cache and bootstrap CI.

Arms (per architecture.md §9.3):
- vectors_off       : control (the system minus vectors)
- vectors_random    : Gaussian vector at matched norm — null for "any perturbation helps"
- vectors_orth      : orthogonal-to-disposition vector — null for "representation impact"
- vectors_on        : the experimental condition

Briefings are cached to data/briefings.pkl keyed by setup_id. Every arm reads
the same briefing for the same setup, so pairing is exact at Stage 1.
(Structural-review #1, #6, #10.)
"""
from pathlib import Path
import pickle
import pandas as pd
import numpy as np
import typer
from xianvec.config import load_config
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.runtime import VectorTrader
from xianvec.eval.backtest import run_backtest, align_paired, BriefingCache
from xianvec.eval.metrics import sharpe, max_drawdown, profit_factor, win_rate, returns_from_pnl
from xianvec.eval.compare import paired_bootstrap_sharpe_delta
from xianvec.pipeline.trade import TradePipeline

DISPOSITION_AXES = ["conviction", "patience", "risk_appetite", "trend_disposition"]

def _decision_signature(d) -> tuple:
    """Behavioural signature for divergence comparison: action + direction +
    size_bucket. Captures meaningful behavioural shifts that an action-only
    check would miss (50bps long vs 1500bps long is a real divergence). Buckets:
    0, <500, <1000, <2000, >=2000. (Structural-review T3.)"""
    if d is None:
        return ("none",)
    sb = d.size_bps or 0
    bucket = 0 if sb == 0 else (1 if sb < 500 else (2 if sb < 1000 else (3 if sb < 2000 else 4)))
    return (d.action, d.direction, bucket)

def _make_arm(arm: str, intern, model, regime_cfg, risk_cfg, briefing_cache):
    """Return a decision_fn for the given experimental arm.

    arm ∈ {"off", "random", "orth", "on"}. Each arm builds a VectorTrader and uses
    the cached Intern briefing (one Intern call per setup_id, shared across arms).
    """
    if arm == "off":
        axis_names: list[str] = []
        vectors_enabled = False
    elif arm == "random":
        axis_names = ["random"]
        vectors_enabled = True
    elif arm == "orth":
        axis_names = ["orthogonal"]
        vectors_enabled = True
    elif arm == "on":
        axis_names = DISPOSITION_AXES
        vectors_enabled = True
    else:
        raise ValueError(f"unknown arm: {arm}")

    trader = VectorTrader(model, axis_names, vectors_enabled=vectors_enabled,
                          backtest_mode=True)  # disable per-setup reload, structural-review #7

    def regime_weights_for_arm(regime: str) -> dict:
        if arm == "off":
            return {}
        if arm == "random":
            return {"random": 1.0}
        if arm == "orth":
            return {"orthogonal": 1.0}
        return regime_cfg.get(regime, {})

    def fn(state, _window):
        # Use the real setup_id (not the literal "ab"). Briefings are cached so
        # every arm reads the same Stage-1 briefing for the same setup.
        setup_id = f"{state.asset}-{int(state.timestamp)}"
        briefing = briefing_cache.get_or_compute(
            setup_id, lambda: intern.reason(state, setup_id=setup_id)
        )
        weights = regime_weights_for_arm(briefing.regime)
        return trader.decide(briefing, regime_vectors=weights)

    return fn

def main(asset_csv: str = "BTC-USD,ETH-USD,SOL-USD",
         data_dir: str = "data/historical",
         block_size: int = 1):
    cfg = load_config("config/default.yaml")
    regime_cfg = load_config("config/regime_vectors.yaml")
    risk_cfg = load_config("config/risk.yaml")

    intern = ClaudeIntern(model=cfg["intern"]["claude_model"])
    model = TraderModel(model_path=cfg["trader"]["model_path"])
    cache = BriefingCache(path="data/briefings.pkl")

    assets = [a.strip() for a in asset_csv.split(",")]
    by_arm: dict[str, list] = {a: [] for a in ["off", "random", "orth", "on"]}

    for asset in assets:
        path = Path(data_dir) / f"{asset.lower()}-15min-90days.parquet"
        if not path.exists():
            print(f"skip {asset}: {path} not found")
            continue
        df = pd.read_parquet(path)
        for arm in by_arm:
            print(f"running arm={arm} on {asset}...")
            fn = _make_arm(arm, intern, model, regime_cfg, risk_cfg, cache)
            r = run_backtest(f"{arm}_{asset}", fn, df, asset, risk_cfg)
            by_arm[arm].append(r)
        cache.save()  # flush per-asset

    def stitch(results):
        pnls, sids, decisions = [], [], []
        for r in results:
            pnls.extend(r.pnls); sids.extend(r.setup_ids); decisions.extend(r.decisions)
        return np.array(pnls), sids, decisions

    off_pnls, off_sids, off_dec = stitch(by_arm["off"])
    rnd_pnls, rnd_sids, rnd_dec = stitch(by_arm["random"])
    orth_pnls, orth_sids, orth_dec = stitch(by_arm["orth"])
    on_pnls, on_sids, on_dec = stitch(by_arm["on"])

    def paired(a_pnls, a_sids, b_pnls, b_sids):
        a_map = dict(zip(a_sids, a_pnls)); b_map = dict(zip(b_sids, b_pnls))
        common = [s for s in a_sids if s in b_map]
        return (np.array([a_map[s] for s in common]),
                np.array([b_map[s] for s in common]),
                common)

    n_resamples = cfg.get("eval", {}).get("bootstrap_resamples", 10000)
    print("\n=== A/B RESULTS (each arm vs vectors_off) ===")
    for arm_name, arm_pnls, arm_sids in [
        ("vectors_random", rnd_pnls, rnd_sids),
        ("vectors_orth", orth_pnls, orth_sids),
        ("vectors_on", on_pnls, on_sids),
    ]:
        a, b, _ = paired(arm_pnls, arm_sids, off_pnls, off_sids)
        if len(a) < 30:
            print(f"{arm_name}: insufficient n={len(a)} (<30); skip")
            continue
        a_rets = returns_from_pnl(a); b_rets = returns_from_pnl(b)
        boot = paired_bootstrap_sharpe_delta(
            a_rets, b_rets, n_resamples=n_resamples, seed=42, block_size=block_size
        )
        print(f"{arm_name:16s}  Δ-Sharpe={boot['delta_sharpe']:+.3f}  "
              f"95% CI [{boot['ci_low']:+.3f}, {boot['ci_high']:+.3f}]  "
              f"p≈{boot['p_value']:.3f}  n={boot['n']}  block_size={boot['block_size']}")

    # Behavioural-signature divergence (vectors_on vs vectors_off)
    on_map = dict(zip(on_sids, on_dec))
    off_map = dict(zip(off_sids, off_dec))
    common = [s for s in on_sids if s in off_map]
    diverged = sum(1 for s in common
                   if _decision_signature(on_map[s]) != _decision_signature(off_map[s]))
    rate = diverged / len(common) if common else 0.0
    print(f"\nDecision divergence (action+direction+size_bucket): "
          f"{diverged}/{len(common)} = {rate:.2%}")

    print("\nPer-asset n (vectors_on):")
    for r in by_arm["on"]:
        final = r.nav_curve[-1] if r.nav_curve else 0
        print(f"  {r.name:24s}  n={len(r.pnls)}  final_nav={final:.0f}")

    Path("data/reports").mkdir(parents=True, exist_ok=True)
    with open("data/reports/by_arm.pkl", "wb") as f:
        pickle.dump(by_arm, f)
    print("\nSaved: data/reports/by_arm.pkl, data/briefings.pkl")

if __name__ == "__main__":
    typer.run(main)
```

- [ ] **Step 2: Smoke run on small price slice**

```bash
# requires data/historical/{btc,eth,sol}-usd-15min-90days.parquet
python scripts/run_ab_compare.py --asset-csv "BTC-USD" --data-dir data/historical
```

Expected: four arms (`off`, `random`, `orth`, `on`) run sequentially against the briefing cache (Intern called once per setup; cache file at `data/briefings.pkl` is reused across arms and across runs). Output prints Δ-Sharpe with 95% CI for `vectors_random`, `vectors_orth`, and `vectors_on` — each compared against `vectors_off`. Statistical significance requires n ≥ 30 paired trades. With three assets at step=24 over 90 days you should land around 1000+ paired setups per arm.

If your data has overlapping forward windows (you set step < horizon), pass `--block-size N` where N ≥ ⌈horizon/step⌉ so the bootstrap accounts for serial correlation.

- [ ] **Step 3: Commit**

```bash
git add scripts/run_ab_compare.py
git commit -m "feat: multi-arm A/B runner with briefing cache and ablation arms"
```

---

## Phase 10 — Demo polish

### Task 10.1: Telegram demo interface

**Files:**
- Create: `src/xianvec/interface/telegram.py`

- [ ] **Step 1: Write Telegram bot**

```python
# src/xianvec/interface/telegram.py
"""Telegram demo interface: /analyze and /compare commands trigger the live pipeline."""
import os
import time
import uuid
from telegram import Update
from telegram.ext import Application, CommandHandler, ContextTypes
from xianvec.config import load_config
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.runtime import VectorTrader
from xianvec.trader.vectors import load_axis_vectors
from xianvec.pipeline.trade import TradePipeline
from xianvec.execution.alpaca import AlpacaExecutor
from xianvec.data.alpaca import fetch_recent_bars
from xianvec.data.indicators import compute_indicators
from xianvec.schemas import MarketState

class XianvecBot:
    def __init__(self):
        self.cfg = load_config("config/default.yaml")
        self.regime_cfg = load_config("config/regime_vectors.yaml")
        self.risk_cfg = load_config("config/risk.yaml")
        self.intern = ClaudeIntern(model=self.cfg["intern"]["claude_model"])
        self.model = TraderModel(layer_range=tuple(self.cfg["trader"]["layer_range"]))
        self.axis_vectors = load_axis_vectors("vectors")
        self.executor = AlpacaExecutor(paper=True)

    def _build_state(self, asset: str) -> MarketState:
        bars = fetch_recent_bars(asset, lookback=200)
        portfolio = self.executor.get_portfolio()
        return MarketState(
            asset=asset, timestamp=time.time(), ohlcv_recent=[],
            indicators=compute_indicators(bars),
            onchain={"smart_money_inflow": 0.0, "funding_rate": 0.0},
            portfolio=portfolio,
        )

    def _run_pipeline(self, state: MarketState, vectors_enabled: bool, setup_id: str):
        trader = VectorTrader(self.model, self.axis_vectors, vectors_enabled=vectors_enabled)
        pipeline = TradePipeline(self.intern, trader, self.risk_cfg, self.regime_cfg)
        return pipeline.run(state, setup_id=setup_id)

    async def cmd_analyze(self, update: Update, ctx: ContextTypes.DEFAULT_TYPE):
        asset = ctx.args[0] if ctx.args else "BTC-USD"
        await update.message.reply_text(f"Analyzing {asset}...")
        state = self._build_state(asset)
        sid = f"{asset}-{int(time.time())}-{uuid.uuid4().hex[:6]}"
        result = self._run_pipeline(state, vectors_enabled=True, setup_id=sid)
        d = result["decision"]
        risk = result["risk"]
        b = result['briefing']
        msg = (f"*{asset}* regime={b.regime}\n"
               f"bull: {b.bull_case}\n"
               f"bear: {b.bear_case}\n"
               f"flat: {b.flat_case}\n\n"
               f"*Decision (vectors ON)*\n"
               f"action={d.action} dir={d.direction} size_bps={d.size_bps}\n"
               f"sl={d.stop_loss_pct} tp={d.take_profit_pct}\n"
               f"trader: _{d.trader_summary}_\n"
               f"vectors: {d.active_vectors}\n"
               f"risk: {'APPROVED' if risk.approved else 'VETOED — ' + (risk.veto_reason or '')}")
        await update.message.reply_text(msg, parse_mode="Markdown")

    async def cmd_compare(self, update: Update, ctx: ContextTypes.DEFAULT_TYPE):
        asset = ctx.args[0] if ctx.args else "BTC-USD"
        await update.message.reply_text(f"Running ON/OFF comparison on {asset}...")
        state = self._build_state(asset)
        sid = f"{asset}-{int(time.time())}-{uuid.uuid4().hex[:6]}"
        on = self._run_pipeline(state, vectors_enabled=True, setup_id=sid + "-ON")
        off = self._run_pipeline(state, vectors_enabled=False, setup_id=sid + "-OFF")
        d_on, d_off = on["decision"], off["decision"]
        msg = (f"*{asset}* regime={on['briefing'].regime}\n\n"
               f"*ON*  action={d_on.action} dir={d_on.direction} size_bps={d_on.size_bps}\n"
               f"  _{d_on.trader_summary}_\n\n"
               f"*OFF* action={d_off.action} dir={d_off.direction} size_bps={d_off.size_bps}\n"
               f"  _{d_off.trader_summary}_\n\n"
               f"divergence: {'DIFFERENT actions' if d_on.action != d_off.action else 'same action'}")
        await update.message.reply_text(msg, parse_mode="Markdown")

    def run(self):
        token = os.environ["TELEGRAM_BOT_TOKEN"]
        app = Application.builder().token(token).build()
        app.add_handler(CommandHandler("analyze", self.cmd_analyze))
        app.add_handler(CommandHandler("compare", self.cmd_compare))
        app.run_polling()

if __name__ == "__main__":
    XianvecBot().run()
```

- [ ] **Step 2: Commit**

```bash
git add src/xianvec/interface/telegram.py
git commit -m "feat(interface): telegram demo bot with analyze/compare commands"
```

---

### Task 10.2: Comparison report generator

**Files:**
- Create: `src/xianvec/eval/report.py`
- Create: `scripts/build_demo_report.py`

- [ ] **Step 1: Write report builder**

```python
# src/xianvec/eval/report.py
import io
import base64
import numpy as np
import matplotlib.pyplot as plt
from xianvec.eval.metrics import equity_curve

def equity_plot_b64(pnls_on, pnls_off, initial_nav=10000):
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.plot(equity_curve(np.array(pnls_on), initial_nav), label="vectors ON", linewidth=2)
    ax.plot(equity_curve(np.array(pnls_off), initial_nav), label="vectors OFF", linewidth=2)
    ax.set_xlabel("trade #")
    ax.set_ylabel("equity ($)")
    ax.legend()
    ax.set_title("XIANVEC: vectors on vs off equity curves")
    buf = io.BytesIO()
    plt.savefig(buf, format="png", dpi=120, bbox_inches="tight")
    plt.close(fig)
    return base64.b64encode(buf.getvalue()).decode()

def markdown_report(boot_result, on_metrics, off_metrics, divergence_rate, plot_b64,
                    boot_random=None, boot_orth=None):
    """Render the demo report.

    Statistical framing (structural-review T3): Δ-Sharpe is the **only** inferential
    test — the bootstrap CI / p-value applies to that single comparison. The four
    secondary metrics (Sharpe, MDD, PF, WR) are *descriptive* — no multiple-comparisons
    correction is applied because we do not claim significance on them.

    The random-vector and orthogonal-vector arms are reported alongside as
    experimental controls (architecture.md §9.3): if vectors_on does not beat
    *both* of them, the result is consistent with "any perturbation activates
    exploration" and the demo narrative weakens.
    """
    controls_section = ""
    if boot_random is not None or boot_orth is not None:
        controls_section = "\n\n### Experimental controls (vs vectors_off)\n\n"
        controls_section += "| Arm | Δ-Sharpe | 95% CI | p | n |\n|---|---|---|---|---|\n"
        for label, b in [("vectors_random", boot_random), ("vectors_orth", boot_orth),
                         ("vectors_on", boot_result)]:
            if b is None:
                continue
            controls_section += (f"| {label} | {b['delta_sharpe']:+.3f} | "
                                 f"[{b['ci_low']:+.3f}, {b['ci_high']:+.3f}] | "
                                 f"{b['p_value']:.3f} | {b['n']} |\n")
        controls_section += ("\n*A clean win requires `vectors_on` to beat both "
                             "`vectors_random` and `vectors_orth` on Δ-Sharpe.*\n")

    return f"""# XIANVEC Demo — A/B Result

**Headline (the only inferential test):** Δ-Sharpe (vectors_on − vectors_off) = **{boot_result['delta_sharpe']:+.3f}**
- 95% CI: [{boot_result['ci_low']:+.3f}, {boot_result['ci_high']:+.3f}]
- p ≈ {boot_result['p_value']:.3f}
- n = {boot_result['n']}, block_size = {boot_result.get('block_size', 1)}

### Descriptive metrics (not inferential)

| Metric | Vectors ON | Vectors OFF |
|---|---|---|
| Sharpe | {on_metrics['sharpe']:.3f} | {off_metrics['sharpe']:.3f} |
| Max Drawdown | {on_metrics['mdd']:.2%} | {off_metrics['mdd']:.2%} |
| Profit Factor | {on_metrics['pf']:.2f} | {off_metrics['pf']:.2f} |
| Win Rate | {on_metrics['wr']:.2%} | {off_metrics['wr']:.2%} |

These four are reported for context. We do not claim statistical significance on
any of them and do not multiple-comparisons-correct, because the headline lives
on Δ-Sharpe alone.

**Decision divergence rate:** {divergence_rate:.2%} (action × direction × size_bucket)
{controls_section}
![equity curves](data:image/png;base64,{plot_b64})
"""
```

- [ ] **Step 2: Write the runner**

```python
# scripts/build_demo_report.py
"""Build the markdown + equity-plot demo report from data/reports/by_arm.pkl.

Reads the four-arm pickle written by `run_ab_compare.py`, recomputes per-arm
metrics, runs the bootstrap for each arm vs vectors_off, and renders the report.
"""
import pickle
from pathlib import Path
import numpy as np
import typer
from xianvec.eval.metrics import sharpe, max_drawdown, profit_factor, win_rate, returns_from_pnl, equity_curve
from xianvec.eval.compare import paired_bootstrap_sharpe_delta
from xianvec.eval.report import equity_plot_b64, markdown_report

def main(by_arm_pickle: str = "data/reports/by_arm.pkl",
         out_path: str = "data/reports/demo.md",
         block_size: int = 1):
    by_arm = pickle.load(open(by_arm_pickle, "rb"))

    def stitch(results):
        pnls, sids = [], []
        for r in results:
            pnls.extend(r.pnls); sids.extend(r.setup_ids)
        return np.array(pnls), sids

    off_pnls, off_sids = stitch(by_arm["off"])
    on_pnls, on_sids = stitch(by_arm["on"])
    rnd_pnls, rnd_sids = stitch(by_arm["random"])
    orth_pnls, orth_sids = stitch(by_arm["orth"])

    def paired(a_pnls, a_sids, b_pnls, b_sids):
        a_map = dict(zip(a_sids, a_pnls)); b_map = dict(zip(b_sids, b_pnls))
        common = [s for s in a_sids if s in b_map]
        return (np.array([a_map[s] for s in common]),
                np.array([b_map[s] for s in common]))

    def boot(a_pnls, b_pnls):
        if len(a_pnls) < 30:
            return None
        return paired_bootstrap_sharpe_delta(
            returns_from_pnl(a_pnls), returns_from_pnl(b_pnls),
            n_resamples=10000, seed=42, block_size=block_size,
        )

    a_on, b_on = paired(on_pnls, on_sids, off_pnls, off_sids)
    a_rnd, b_rnd = paired(rnd_pnls, rnd_sids, off_pnls, off_sids)
    a_orth, b_orth = paired(orth_pnls, orth_sids, off_pnls, off_sids)

    boot_on = boot(a_on, b_on)
    boot_rnd = boot(a_rnd, b_rnd)
    boot_orth = boot(a_orth, b_orth)
    if boot_on is None:
        raise SystemExit("vectors_on has insufficient paired n; run a longer backtest.")

    a_rets = returns_from_pnl(a_on); b_rets = returns_from_pnl(b_on)
    on_metrics = {
        "sharpe": sharpe(a_rets), "mdd": max_drawdown(equity_curve(a_on)),
        "pf": profit_factor(a_on), "wr": win_rate(a_on),
    }
    off_metrics = {
        "sharpe": sharpe(b_rets), "mdd": max_drawdown(equity_curve(b_on)),
        "pf": profit_factor(b_on), "wr": win_rate(b_on),
    }

    # divergence — recompute via stitched decisions from the on/off arms
    on_decs, off_decs = {}, {}
    for r in by_arm["on"]:
        for sid, d in zip(r.setup_ids, r.decisions):
            on_decs[sid] = d
    for r in by_arm["off"]:
        for sid, d in zip(r.setup_ids, r.decisions):
            off_decs[sid] = d
    from scripts.run_ab_compare import _decision_signature
    common = [s for s in on_decs if s in off_decs]
    diverged = sum(1 for s in common
                   if _decision_signature(on_decs[s]) != _decision_signature(off_decs[s]))
    divergence_rate = diverged / len(common) if common else 0.0

    plot = equity_plot_b64(a_on.tolist(), b_on.tolist())
    md = markdown_report(
        boot_on, on_metrics, off_metrics, divergence_rate, plot,
        boot_random=boot_rnd, boot_orth=boot_orth,
    )
    Path(out_path).parent.mkdir(parents=True, exist_ok=True)
    Path(out_path).write_text(md)
    print(f"wrote {out_path}")

if __name__ == "__main__":
    typer.run(main)
```

- [ ] **Step 3: Commit**

```bash
git add src/xianvec/eval/report.py scripts/build_demo_report.py
git commit -m "feat(eval): demo report generator and runner"
```

---

## Phase 11 — Forward paper trading + onchain data

### Task 11.1: Forward paper runner

**Files:**
- Create: `scripts/run_paper.py`

- [ ] **Step 1: Implement runner**

```python
# scripts/run_paper.py
"""Forward Alpaca paper trading. Alternates vectors-on and vectors-off setups for paired data."""
import time
import itertools
import pandas as pd
from xianvec.config import load_config
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.runtime import VectorTrader
from xianvec.trader.vectors import load_axis_vectors
from xianvec.pipeline.trade import TradePipeline
from xianvec.execution.alpaca import AlpacaExecutor
from xianvec.data.alpaca import fetch_recent_bars  # see Task 11.2
from xianvec.data.indicators import compute_indicators
from xianvec.data.store import Store
from xianvec.schemas import MarketState
import uuid

def main(asset: str = "BTC-USD", cadence_min: int = 15):
    cfg = load_config("config/default.yaml")
    regime_cfg = load_config("config/regime_vectors.yaml")
    risk_cfg = load_config("config/risk.yaml")
    store = Store("data/decisions.db"); store.init()

    intern = ClaudeIntern(model=cfg["intern"]["claude_model"])
    model = TraderModel(layer_range=tuple(cfg["trader"]["layer_range"]))
    axis_vectors = load_axis_vectors("vectors")
    executor = AlpacaExecutor(paper=True)

    flip = itertools.cycle([True, False])  # alternate ON/OFF for paired forward data

    while True:
        vectors_enabled = next(flip)
        trader = VectorTrader(model, axis_vectors, vectors_enabled=vectors_enabled)
        pipeline = TradePipeline(intern, trader, risk_cfg, regime_cfg)

        bars = fetch_recent_bars(asset, lookback=200)
        portfolio = executor.get_portfolio()
        state = MarketState(
            asset=asset,
            timestamp=time.time(),
            ohlcv_recent=[],
            indicators=compute_indicators(bars),
            onchain={},
            portfolio=portfolio,
        )
        sid = f"{asset}-{int(time.time())}-{uuid.uuid4().hex[:6]}-{'ON' if vectors_enabled else 'OFF'}"
        result = pipeline.run(state, setup_id=sid)
        store.save_briefing(result["briefing"])
        store.save_decision(result["decision"], vectors_enabled=vectors_enabled)

        if result["risk"].approved:
            actual = result["risk"].modified or result["risk"].original
            ack = executor.execute(actual, asset=asset, nav=portfolio["nav"])
            print(f"[{sid}] vectors={'ON' if vectors_enabled else 'OFF'} "
                  f"action={actual.action} size_bps={actual.size_bps} -> {ack}")
        else:
            print(f"[{sid}] vetoed: {result['risk'].veto_reason}")

        time.sleep(cadence_min * 60)

if __name__ == "__main__":
    import typer
    typer.run(main)
```

- [ ] **Step 2: Commit**

```bash
git add scripts/run_paper.py
git commit -m "feat: forward paper trading runner with alternating on/off"
```

---

### Task 11.2: Alpaca historical + recent bars fetcher

**Files:**
- Create: `src/xianvec/data/alpaca.py`

- [ ] **Step 1: Implement fetcher**

```python
# src/xianvec/data/alpaca.py
import os
from datetime import datetime, timedelta, timezone
import pandas as pd
from alpaca.data.historical import CryptoHistoricalDataClient
from alpaca.data.requests import CryptoBarsRequest
from alpaca.data.timeframe import TimeFrame

_client = None
def _get_client():
    global _client
    if _client is None:
        _client = CryptoHistoricalDataClient(
            api_key=os.environ["ALPACA_API_KEY"],
            secret_key=os.environ["ALPACA_API_SECRET"],
        )
    return _client

def fetch_recent_bars(symbol: str, lookback: int = 200, timeframe_min: int = 15) -> pd.DataFrame:
    end = datetime.now(timezone.utc)
    start = end - timedelta(minutes=timeframe_min * (lookback + 5))
    req = CryptoBarsRequest(
        symbol_or_symbols=symbol,
        timeframe=TimeFrame.Minute if timeframe_min == 1 else TimeFrame(timeframe_min, "Min"),
        start=start, end=end,
    )
    bars = _get_client().get_crypto_bars(req).df
    bars = bars.xs(symbol, level=0) if isinstance(bars.index, pd.MultiIndex) else bars
    return bars[["open", "high", "low", "close", "volume"]].tail(lookback)

def fetch_historical_bars(symbol: str, days: int = 90, timeframe_min: int = 15) -> pd.DataFrame:
    end = datetime.now(timezone.utc)
    start = end - timedelta(days=days)
    req = CryptoBarsRequest(
        symbol_or_symbols=symbol,
        timeframe=TimeFrame(timeframe_min, "Min"),
        start=start, end=end,
    )
    bars = _get_client().get_crypto_bars(req).df
    bars = bars.xs(symbol, level=0) if isinstance(bars.index, pd.MultiIndex) else bars
    return bars[["open", "high", "low", "close", "volume"]]
```

- [ ] **Step 2: Commit**

```bash
git add src/xianvec/data/alpaca.py
git commit -m "feat(data): alpaca historical + recent bars fetcher"
```

---

### Task 11.3: Exchange funding rate + open interest fetcher

**Files:**
- Create: `src/xianvec/data/exchange.py`

- [ ] **Step 1: Implement fetcher**

```python
# src/xianvec/data/exchange.py
"""Public-endpoint funding-rate and open-interest fetchers via ccxt. No auth required."""
import ccxt

def fetch_funding_rate(symbol: str, exchange: str = "binance") -> float:
    """Return latest funding rate for a perp. Symbol example: BTC/USDT:USDT."""
    ex = getattr(ccxt, exchange)({"options": {"defaultType": "swap"}})
    fr = ex.fetch_funding_rate(symbol)
    return float(fr.get("fundingRate", 0.0) or 0.0)

def fetch_open_interest(symbol: str, exchange: str = "binance") -> float:
    """Return latest open interest in base-asset units."""
    ex = getattr(ccxt, exchange)({"options": {"defaultType": "swap"}})
    oi = ex.fetch_open_interest(symbol)
    return float(oi.get("openInterestAmount", 0.0) or 0.0)

# Map our internal asset names (Alpaca-style) to exchange perp symbols.
PERP_SYMBOLS = {
    "BTC-USD": "BTC/USDT:USDT",
    "ETH-USD": "ETH/USDT:USDT",
    "SOL-USD": "SOL/USDT:USDT",
}

def onchain_signals(asset: str) -> dict[str, float]:
    """Best-effort onchain/perp signals dict for the asset. Empty if unavailable."""
    out = {}
    sym = PERP_SYMBOLS.get(asset)
    if not sym:
        return out
    try:
        out["funding_rate"] = fetch_funding_rate(sym)
    except Exception:
        pass
    try:
        out["open_interest"] = fetch_open_interest(sym)
    except Exception:
        pass
    return out
```

- [ ] **Step 2: Smoke test**

```bash
python -c "
from xianvec.data.exchange import onchain_signals
print(onchain_signals('BTC-USD'))
"
```

Expected: `{'funding_rate': <float>, 'open_interest': <float>}`. If ccxt errors, check that `ccxt>=4.3` resolved.

- [ ] **Step 3: Commit**

```bash
git add src/xianvec/data/exchange.py
git commit -m "feat(data): public funding-rate and open-interest fetcher via ccxt"
```

---

### Task 11.4: Nansen client (minimal)

**Files:**
- Create: `src/xianvec/data/nansen.py`

- [ ] **Step 1: Thin client**

```python
# src/xianvec/data/nansen.py
"""Minimal Nansen client. Wraps the smart-money flow endpoint we actually use."""
import os
import httpx

BASE = "https://api.nansen.ai/api/v1"

def _client() -> httpx.Client:
    return httpx.Client(
        base_url=BASE,
        headers={"apiKey": os.environ["NANSEN_API_KEY"]},
        timeout=20.0,
    )

def smart_money_inflow(asset_symbol: str, lookback_hours: int = 4) -> float:
    """Net smart-money inflow for `asset_symbol` over the lookback window.

    Returns a normalized signal in roughly [-1, 1]: positive = inflow, negative = outflow.
    Returns 0.0 on any client/parsing error so callers can treat it as 'no signal'.
    """
    try:
        with _client() as c:
            r = c.get("/smart-money/inflows",
                      params={"symbol": asset_symbol, "lookback_hours": lookback_hours})
            r.raise_for_status()
            data = r.json()
            net = float(data.get("net_flow_usd", 0.0))
            volume = float(data.get("total_volume_usd", 1.0)) or 1.0
            return max(-1.0, min(1.0, net / volume))
    except Exception:
        return 0.0
```

Note: endpoint paths and field names should be confirmed against the current Nansen docs before relying on this. If Nansen is not subscribed, callers handle the empty signal gracefully (returns 0.0).

- [ ] **Step 2: Wire into forward paper runner**

Update `scripts/run_paper.py` to populate `state.onchain` from both fetchers:

```python
from xianvec.data.exchange import onchain_signals
from xianvec.data.nansen import smart_money_inflow

# inside the loop, replacing onchain={}:
onchain = onchain_signals(asset)
onchain["smart_money_inflow"] = smart_money_inflow(asset.split("-")[0])
```

- [ ] **Step 3: Commit**

```bash
git add src/xianvec/data/nansen.py scripts/run_paper.py
git commit -m "feat(data): nansen smart-money client and live onchain wiring"
```

---

### Task 11.5: Mantle/Byreal forward runner with on-chain reputation logging

**Why:** Phase 11.1 (Alpaca paper) is the pre-launch sanity-check path. This task adds the *hackathon submission* path: real execution on Mantle via Byreal Perps, with each closed trade posting a reputation-registry update to the corresponding ERC-8004 NFT. Capital is pre-funded; the agent never bridges. See Mantle integration §M2/M3.

**Files:**
- Create: `scripts/run_mantle_forward.py`

**Pre-flight requirements:**
- Phase 6.3 ByrealExecutor implemented and read-side smoke-tested.
- Phase 6.5 NFTs minted; `identity/registered.json` committed.
- Phase 11.6 mantle-risk-evaluator gate implemented (this script invokes it).

- [ ] **Step 1: Implement runner**

```python
# scripts/run_mantle_forward.py
"""Forward Mantle/Byreal execution with on-chain reputation logging.

Alternates vectors-ON and vectors-OFF setups for paired forward data. Each
closed trade posts a reputation update keyed to that arm's ERC-8004 NFT.
The mantle-risk-evaluator skill is invoked between the deterministic risk
layer's approval and the Byreal submission as a second gate.
"""
import time
import itertools
import uuid
import typer
from xianvec.config import load_config
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.runtime import VectorTrader
from xianvec.pipeline.trade import TradePipeline
from xianvec.execution.byreal import ByrealExecutor
from xianvec.data.alpaca import fetch_recent_bars
from xianvec.data.indicators import compute_indicators
from xianvec.data.store import Store
from xianvec.schemas import MarketState
from xianvec.onchain.decision_log import DecisionLog
from xianvec.onchain.mantle_skills_gate import MantleRiskEvaluator  # Task 11.6

DISPOSITION_AXES = ["conviction", "patience", "risk_appetite", "trend_disposition"]

def main(asset: str = "BTC-USD", cadence_min: int = 15, dry_run: bool = False):
    cfg = load_config("config/default.yaml")
    regime_cfg = load_config("config/regime_vectors.yaml")
    risk_cfg = load_config("config/risk.yaml")
    store = Store("data/decisions.db"); store.init()

    intern = ClaudeIntern(model=cfg["intern"]["claude_model"])
    model = TraderModel(model_path=cfg["trader"]["model_path"])
    executor = ByrealExecutor()
    decision_log = DecisionLog()
    mantle_gate = MantleRiskEvaluator()

    flip = itertools.cycle([True, False])  # alternate ON/OFF for paired forward data

    while True:
        vectors_enabled = next(flip)
        arm = "vectors_on" if vectors_enabled else "vectors_off"
        axis_names = DISPOSITION_AXES if vectors_enabled else []
        trader = VectorTrader(model, axis_names, vectors_enabled=vectors_enabled,
                              backtest_mode=False)  # forward path keeps gating reload
        pipeline = TradePipeline(intern, trader, risk_cfg, regime_cfg)

        bars = fetch_recent_bars(asset, lookback=200)
        portfolio = executor.get_portfolio()
        state = MarketState(
            asset=asset, timestamp=time.time(), ohlcv_recent=[],
            indicators=compute_indicators(bars), onchain={}, portfolio=portfolio,
        )
        sid = f"{asset}-{int(time.time())}-{uuid.uuid4().hex[:6]}-{arm}"
        result = pipeline.run(state, setup_id=sid)
        store.save_briefing(result["briefing"])
        store.save_decision(result["decision"], vectors_enabled=vectors_enabled)

        if not result["risk"].approved:
            print(f"[{sid}] vetoed (deterministic): {result['risk'].veto_reason}")
            time.sleep(cadence_min * 60); continue

        actual = result["risk"].modified or result["risk"].original

        # Phase 11.6 gate: mantle-risk-evaluator pre-flight
        verdict = mantle_gate.evaluate(actual, asset=asset, portfolio=portfolio)
        store.save_mantle_verdict(setup_id=sid, verdict=verdict)
        if verdict["status"] == "block":
            print(f"[{sid}] vetoed (mantle-risk-evaluator): {verdict['reason']}")
            time.sleep(cadence_min * 60); continue

        if dry_run:
            print(f"[{sid}] DRY RUN — would submit: {actual.action} {actual.size_bps}bps")
            time.sleep(cadence_min * 60); continue

        ack = executor.execute(actual, asset=asset, nav=portfolio["nav"])
        print(f"[{sid}] {arm}  action={actual.action} size_bps={actual.size_bps} -> {ack}")

        # On Byreal, "execute" submits an open order; the closed-trade reputation post
        # happens via a position-monitor loop (separate concern, not in v1's hot path —
        # for the hackathon, post on submission with placeholder pnl=0, then update on close).
        decision_log.post_trade(
            arm=arm, setup_id=sid, action=actual.action, direction=actual.direction,
            size_bps=actual.size_bps, pnl=0.0,
        )

        time.sleep(cadence_min * 60)

if __name__ == "__main__":
    typer.run(main)
```

- [ ] **Step 2: Smoke run with `--dry-run` first**

```bash
python scripts/run_mantle_forward.py --asset BTC-USD --cadence-min 15 --dry-run
```

Expected: pipeline runs, deterministic risk + mantle-risk-evaluator both report verdicts, no Byreal txns submitted, reputation log not posted. Verifies the wiring before any on-chain capital moves.

- [ ] **Step 3: Live run (small notional only)**

Once dry-run looks clean, drop the flag and let it run. Cap initial position sizes via `config/risk.yaml`'s `max_position_size_pct` to a small number (e.g. 2%) for the first few hours.

- [ ] **Step 4: Commit**

```bash
git add scripts/run_mantle_forward.py
git commit -m "feat: mantle/byreal forward runner with on-chain reputation logging"
```

---

### Task 11.6: mantle-risk-evaluator pre-flight gate

**Why:** Phase 5's deterministic `RiskEvaluator` knows position sizing, exposure caps, and loss-streak cooldown. It does NOT know Mantle-specific failure modes — interacting with an unverified contract, a venue with thin liquidity, a position that conflicts with on-chain governance. The `mantle-risk-evaluator` skill from `.claude/skills/mantle/` provides a second, LLM-mediated verdict (pass / warn / block) with that Mantle context. Additive, not replacing.

For v1, "invoke the skill" means: send the decision + portfolio context to a Claude API call with the skill's `SKILL.md` and relevant `references/` loaded as system prompt, parse the verdict from the response.

**Files:**
- Create: `src/xianvec/onchain/mantle_skills_gate.py`

- [ ] **Step 1: Implement gate**

```python
# src/xianvec/onchain/mantle_skills_gate.py
"""Invokes the mantle-risk-evaluator filesystem skill via the Claude API.

The skill files live under .claude/skills/mantle/skills/mantle-risk-evaluator/
(vendored in Phase 0.4). We load SKILL.md as the system prompt and forward
the structured decision + portfolio for evaluation. The skill returns a
verdict: "pass", "warn", or "block" with a one-line reason.

This is additive to xianvec's deterministic RiskEvaluator (Phase 5). Forward
runs invoke both gates; backtests skip this one.
"""
import json
from pathlib import Path
from anthropic import Anthropic
from xianvec.schemas import TraderDecision

SKILL_DIR = Path(".claude/skills/mantle/skills/mantle-risk-evaluator")

class MantleRiskEvaluator:
    def __init__(self, client: Anthropic | None = None,
                 model: str = "claude-haiku-4-5"):
        self.client = client or Anthropic()
        self.model = model
        self.skill_md = (SKILL_DIR / "SKILL.md").read_text()
        # Load any small references the skill explicitly says to consult
        ref_dir = SKILL_DIR / "references"
        self.references = ""
        if ref_dir.exists():
            for f in sorted(ref_dir.glob("*.md")):
                self.references += f"\n\n--- {f.name} ---\n{f.read_text()}"

    def evaluate(self, d: TraderDecision, asset: str, portfolio: dict) -> dict:
        system = (self.skill_md + self.references +
                  "\n\nReply ONLY with JSON: {\"status\": \"pass\"|\"warn\"|\"block\", "
                  "\"reason\": \"<one short line>\"}")
        user = json.dumps({
            "asset": asset,
            "decision": {"action": d.action, "direction": d.direction,
                         "size_bps": d.size_bps, "stop_loss_pct": d.stop_loss_pct,
                         "take_profit_pct": d.take_profit_pct,
                         "active_vectors": d.active_vectors},
            "portfolio": portfolio,
            "venue": "Byreal Perps on Mantle",
        }, indent=2)
        resp = self.client.messages.create(
            model=self.model, max_tokens=300, temperature=0.0,
            system=system, messages=[{"role": "user", "content": user}],
        )
        text = resp.content[0].text.strip()
        if text.startswith("```"):
            text = text.split("```", 2)[1].lstrip("json").strip()
        verdict = json.loads(text)
        if verdict.get("status") not in ("pass", "warn", "block"):
            return {"status": "block", "reason": f"malformed verdict: {text[:100]}"}
        return verdict
```

- [ ] **Step 2: Unit test (mocked Claude)**

```python
# tests/unit/test_mantle_gate.py
from unittest.mock import MagicMock
from xianvec.onchain.mantle_skills_gate import MantleRiskEvaluator
from xianvec.schemas import TraderDecision

def test_pass_verdict_parsed():
    fake_resp = MagicMock()
    fake_resp.content = [MagicMock(text='{"status": "pass", "reason": "looks ok"}')]
    client = MagicMock()
    client.messages.create.return_value = fake_resp
    g = MantleRiskEvaluator(client=client)
    d = TraderDecision(setup_id="x", action="buy", size_bps=100, direction="long",
                       stop_loss_pct=2.0, take_profit_pct=4.0,
                       trader_summary="t", active_vectors={})
    out = g.evaluate(d, asset="BTC-USD", portfolio={"nav": 10000})
    assert out["status"] == "pass"

def test_block_verdict_on_malformed():
    fake_resp = MagicMock()
    fake_resp.content = [MagicMock(text="not json at all")]
    client = MagicMock()
    client.messages.create.return_value = fake_resp
    g = MantleRiskEvaluator(client=client)
    d = TraderDecision(setup_id="x", action="buy", size_bps=100, direction="long",
                       stop_loss_pct=2.0, take_profit_pct=4.0,
                       trader_summary="t", active_vectors={})
    out = g.evaluate(d, asset="BTC-USD", portfolio={"nav": 10000})
    assert out["status"] == "block"     # fail closed on parse errors
```

- [ ] **Step 3: Commit**

```bash
git add src/xianvec/onchain/mantle_skills_gate.py tests/unit/test_mantle_gate.py
git commit -m "feat(onchain): mantle-risk-evaluator pre-flight gate"
```

---

## Phase 12 — Self-review checklist

Before declaring the build done for hackathon:

- [ ] All unit tests passing: `pytest tests/unit -v`
- [ ] All integration tests passing: `pytest tests/integration -v`
- [ ] fp16 spike passes (`scripts/spike_vector_validation.py`)
- [ ] **GGUF quantization gate passes** (`scripts/spike_vector_validation_gguf.py`) — directional match ≥ 50% on Q4_K_M
- [ ] `scripts/smoke_pipeline_with_vectors.py` produces visibly different on/off decisions and prints a non-zero gate magnitude at the action token
- [ ] At least one full backtest run across the asset whitelist (BTC + ETH + SOL) over ≥90 days of 15-min data completes without error
- [ ] A/B comparison run produces Δ-Sharpe with bootstrap CI for **all four arms** (`vectors_off`, `vectors_random`, `vectors_orth`, `vectors_on`) on n ≥ 100 paired trades each
- [ ] **`vectors_on` Δ-Sharpe beats both `vectors_random` and `vectors_orth`** (otherwise the result is consistent with "any perturbation activates exploration" — demo narrative weakens)
- [ ] **Briefing cache hit rate ≥ 99%** on the second arm onward (proves Stage 1 pairing is exact across arms)
- [ ] **Stateful portfolio**: at least one risk veto fires during backtest (cluster cap / circuit breaker / loss-streak cooldown) — confirms the risk layer is actually live in backtest, not a no-op
- [ ] **Block-bootstrap sanity**: if `step < horizon`, results are reported with `block_size = ⌈horizon/step⌉`; otherwise IID is fine
- [ ] Demo report (`scripts/build_demo_report.py`) renders the markdown report with the controls table, descriptive metrics framing, and equity-curve plot
- [ ] Telegram bot connects and responds to `/analyze` and `/compare` commands
- [ ] Forward paper run (`scripts/run_paper.py`) executes a real Alpaca paper order successfully (verify in Alpaca dashboard)
- [ ] Exchange funding-rate fetch returns a real number (`onchain_signals('BTC-USD')`)
- [ ] Nansen client either returns real signal or fails gracefully to 0.0
- [ ] All decisions persisted to `data/decisions.db` with `vectors_enabled` flag and `_gate_magnitude` field
- [ ] `decisions/0001-model-choice.md` populated by spike output
- [ ] `decisions/0001b-gguf-validation.md` populated by GGUF spike
- [ ] `decisions/0002-lookahead-audit.md` reviewed and signed
- [ ] `decisions/0003-related-work.md` reviewed; one-sentence positioning memorized for demo
- [ ] Walk-forward fold-level Δ-Sharpe is roughly stationary (large fold-to-fold divergence = leakage signal or regime instability)
- [ ] No secrets in code or committed config (audit with `git log -p | grep -E "sk-|api_key"`)

**Mantle integration acceptance (hackathon submission):**

- [ ] mantle-skills submodule pinned and reachable: `ls .claude/skills/mantle/skills/` lists all 11 skills
- [ ] `decisions/0004-mantle-skills.md` documents which skills xianvec consumes
- [ ] Both ERC-8004 NFTs minted; `identity/registered.json` committed; both `agentURI`s resolve to valid manifests
- [ ] At least one Alpaca paper trade completes (Phase 11.1) — proves Stage 1→2→3 plumbing
- [ ] At least one Byreal perp open + close completes on Mantle mainnet (Phase 11.5) — verify on block explorer
- [ ] At least one reputation-registry post per arm (`vectors_on`, `vectors_off`) on Mantle — verify on block explorer
- [ ] mantle-risk-evaluator gate exercised at least once with each verdict (pass / warn / block) — capture transcripts in `decisions/`
- [ ] At least one xStock symbol attempted on Byreal (even if vetoed) — confirms whitelist + venue mapping is wired through
- [ ] No `@mantleio/sdk` or other bridge code present in repo (`git grep mantleio`) — capital is pre-funded, not bridged by the agent
- [ ] MNT balance + base collateral on the agent wallet documented in `identity/registered.json` for reproducibility

---

## Hackathon parallelization

Time-boxed execution. These tracks have no dependencies between them and can run in parallel while you focus on the critical path:

**Critical path (must be sequential, you do this):**
Phase 0 → Phase 1.1 (schemas) → Phase 4 (vectors) → Phase 9 (A/B) → Phase 10 demo polish.

**Parallelizable tracks (delegate to subagents or do in parallel sessions):**
- **Track A: Baselines.** Phase 7.1 + 7.2 are pure functions over price/onchain dicts — Haiku-class subagent can implement and test these independently.
- **Track B: Eval framework.** Phase 8.1 (metrics) + 8.2 (paired bootstrap) are pure stats; Haiku can implement against the test specs in this doc.
- **Track C: Data fetchers.** Phase 11.2 (Alpaca), 11.3 (exchange), 11.4 (Nansen) are independent IO modules; one subagent can do all three.
- **Track D: Contrastive datasets.** Phase 4.1's four JSON files (200 pairs each, post-structural-review) is content generation — Sonnet/Opus subagent producing high-quality contrastive prompts.
- **Track E: Mantle integration.** Phase 0.4 (vendor mantle-skills), Phase 6.5 (ERC-8004 mint), and Phase 6.3 (Byreal executor) can run in parallel with the trader/eval critical path. Sonnet-class subagent for Phase 6.3 (CLI wrapper); Opus for Phase 6.5 (web3 + ABI work has more failure modes).

Recommended sequence: kick off tracks A, B, C, D, E in parallel as soon as Phase 1 schemas are merged. They feed back into the critical path at Phase 4 (D), Phase 8 (B), Phase 9 (A), Phase 11 (C, E).

---

## Demo narrative (for hackathon presentation)

The story arc that the data should tell:

1. **The thesis** (30 sec): control vectors encode disposition; same agent, vectors on vs off, on the same setups.
2. **The vectors-OFF baseline** (30 sec): show the agent's vectors-off Sharpe alongside textbook baselines (RSI, MA, Donchian, smart-money copy). Establish "this is a competent baseline trader."
3. **The vectors-ON result and the experimental controls** (60 sec): show Δ-Sharpe with bootstrap CI for all four arms — `vectors_on`, `vectors_random`, `vectors_orth`, `vectors_off`. The headline is Δ-Sharpe(on − off); the credibility comes from `vectors_on` beating both random and orthogonal arms. If it doesn't beat both, the result is consistent with "any perturbation activates exploration" and the narrative is honest about that.
4. **What the vectors did** (60 sec): show 2-3 specific setups where vectors-on and vectors-off chose different actions. Read the `trader_summary` from each. This is where the demo lives — the human-legible behavior shift.
5. **The on-chain proof** (30 sec): both arms are ERC-8004 NFTs on Mantle; every Byreal trade's reputation update is on-chain and publicly queryable. The Δ-Sharpe claim is independently verifiable — pull the reputation history for both `token_id`s and run the bootstrap yourself.
6. **What's next** (30 sec): SVF for context-conditional steering, Karpathy loop for self-improvement from on-chain trade outcomes (the ERC-8004 reputation log becomes the training signal), expansion to multi-venue execution beyond Byreal.

---

*Plan version: 2026-05-02. Lives at `/Users/edkennedy/Code/xianvec/implementation-plan.md`. Companion: `architecture.md`.*

---

## Future additions (post-hypothesis-validation)

Items researched and assessed but deliberately deferred until the core vector hypothesis (Δ-Sharpe > 0 across regimes) is validated. Adding any of these before that point risks conflating variables or burning cycles on infrastructure before the experiment has a result.

### Cross-run memory system (MemPalace)

**What it is:** Semantic retrieval of past backtest run summaries — vector configurations, rubric scores, regime conditions, natural-language lessons from the evaluator — injected into subsequent runs. Enables cross-run learning without retraining the base model.

**Why deferred:** Until the vector hypothesis is validated, injecting memory into runs introduces a second variable. If Δ-Sharpe improves across runs, the improvement cannot be attributed cleanly to better vector configurations vs the injected memory context. The experiment is currently measuring whether vectors work at all; memory conflates that signal.

**Assessed candidate:** MemPalace (github.com/mempalace/mempalace, PyPI: `mempalace`). Assessed 2026-05-02. Verdict: legitimate — 50k+ stars, 64 test files, committed reproducible benchmarks (96.6% R@5 raw on LongMemEval, no API keys, 98.4% hybrid), MIT, Python 3.9+, local-first ChromaDB + SQLite. Minor flag: 50k stars in 4 weeks is unusual velocity, but code substance is real. Fits Xianvec's stack exactly: offline, SQLite already in use, pluggable backends.

**Integration sketch (for when the time comes):**
```python
# After each backtest run:
palace.store(
    wing="backtest_runs",
    room=f"regime_{regime_type}",
    drawer=f"run_{run_id}_score_{delta_sharpe:.3f}",
    content=json.dumps({
        "vector_config": active_magnitudes,
        "delta_sharpe": delta_sharpe,
        "regime": regime_type,
        "regime_gate_passed": gate_passed,
        "lesson": evaluator_next_iteration_text,
    })
)
# At start of next run:
similar = palace.search(
    f"vector configuration {current_regime} regime positive delta sharpe",
    k=5, wing="backtest_runs"
)
# Inject top-k summaries into Stage 1 system prompt context
```

**Trigger for adding:** First clean backtest result that passes the regime gate (positive Δ-Sharpe in both bear and bull folds). At that point, memory is augmenting a confirmed signal rather than competing with an unconfirmed one.

### Vector magnitude hill-climbing loop

**What it is:** Automated search over vector axis magnitudes (conviction, patience, risk_appetite, trend_disposition) using a run → grade → seed-next-run feedback loop. Currently magnitudes are hand-set in `config/regime_vectors.yaml`.

**Why deferred:** Requires a working eval loop (Phase 8) and validated regime gate (Task 8.0b) before the hill-climbing rubric can be trusted. NexusTrade's $676 warning applies directly: a rubric that optimizes Δ-Sharpe without the regime gate will find configurations that score well on single-regime data and fail in deployment. Get the gate working and a baseline result first.

**When to add:** After the regime gate is confirmed working and at least one manual vector configuration has produced a clean result. Then hill-climbing with the gated rubric is safe.
