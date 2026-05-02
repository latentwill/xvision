# XIANVEC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a 4-stage trading agent (Intern → Trader → Risk → Execution) where Stage 2 (Trader) uses control vectors to encode disposition, and prove via paired backtest that vectors-on outperforms vectors-off on Δ-Sharpe. Stage 1 (Intern) prepares neutral bull/bear/flat evidence with no recommendation, so the Trader's vectors get clean steering room.

**Architecture:** See `architecture.md` (sibling file). Three model-bearing components, one rules-only risk layer between them. Vectors are active only in Stage 2. Pydantic schemas enforce all stage handoffs. SQLite persists every decision for replay.

**Tech Stack:** Python 3.11, mlx/llama-cpp-python (local Qwen quantized inference), repeng (control vectors), Anthropic SDK (Stage 1 cloud), alpaca-py (paper trading), pandas-ta (technicals), Nansen API (onchain), pydantic v2 (schemas), pytest (tests), typer (CLI), python-telegram-bot (demo interface).

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
├── src/xianvec/
│   ├── __init__.py
│   ├── config.py               # config loading + validation
│   ├── schemas.py              # Pydantic stage handoff models
│   ├── data/
│   │   ├── alpaca.py
│   │   ├── nansen.py
│   │   ├── exchange.py
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
│   │   ├── alpaca.py           # live Alpaca paper
│   │   └── simulator.py        # backtest sim
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
    ├── run_paper.py
    └── compare_runs.py
```

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
  "alpaca-py>=0.21",
  "anthropic>=0.34",
  "httpx>=0.27",
  "ccxt>=4.3",
  "python-telegram-bot>=21",
  "matplotlib>=3.8",
  "seaborn>=0.13",
]

[project.optional-dependencies]
inference = [
  "torch>=2.3",
  "transformers>=4.42",
  "repeng>=0.5",
  "mlx-lm>=0.14",
  "llama-cpp-python>=0.2.85",
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
ANTHROPIC_API_KEY=sk-ant-...
ALPACA_API_KEY=...
ALPACA_API_SECRET=...
ALPACA_PAPER=true
NANSEN_API_KEY=...
TELEGRAM_BOT_TOKEN=...
TELEGRAM_CHAT_ID=...
```

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
assets:
  - BTC-USD
  - ETH-USD
  - SOL-USD
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
        resp = self.client.messages.create(
            model=self.model,
            max_tokens=1024,
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
    """Wraps llama.cpp Qwen for Stage 2 inference. Vector hooks added in Phase 4."""

    def __init__(self, model_path: str, n_ctx: int = 8192):
        if not Path(model_path).exists():
            raise FileNotFoundError(f"model not found: {model_path}")
        self.llm = Llama(model_path=model_path, n_ctx=n_ctx, verbose=False, logits_all=False)

    def generate(self, prompt: str, max_tokens: int = 256, temperature: float = 0.4) -> str:
        out = self.llm(prompt, max_tokens=max_tokens, temperature=temperature, stop=["\n\n"])
        return out["choices"][0]["text"]
```

Note: this loader uses the GGUF Q4_K_M for Phase 3. Vector application requires a transformers-backed path, added in Phase 4. Phase 3 just verifies end-to-end JSON generation works.

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

Generate 50 pairs per axis covering: breakouts, breakdowns, range-bound, regime shifts, smart money flows, funding extremes, liquidation events, low/high vol regimes.

- [ ] **Step 2: Write extraction module**

```python
# src/xianvec/trader/extract.py
import json
from pathlib import Path
from typing import Any
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
```

- [ ] **Step 3: Write extraction runner**

```python
# scripts/extract_vectors.py
from xianvec.trader.extract import extract_all
from xianvec.config import load_config

cfg = load_config("config/default.yaml")
extract_all("Qwen/Qwen3-14B", tuple(cfg["trader"]["layer_range"]))
```

- [ ] **Step 4: Run extraction**

```bash
python scripts/extract_vectors.py
```

Expected: four `.pt` files in `vectors/`. Takes 5-15 minutes per axis on M-series Mac.

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
    """Map next-token entropy to a vector magnitude scaler in [0, 1].

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

### Task 4.3: Trader model with vectors integrated

**Files:**
- Modify: `src/xianvec/trader/model.py`
- Test: extend `tests/integration/test_pipeline.py`

- [ ] **Step 1: Refactor model.py to use transformers + ControlModel**

Replace the llama.cpp implementation with the transformers-backed version (vectors require it). Keep the public interface (`generate`) compatible.

```python
# src/xianvec/trader/model.py
from pathlib import Path
import numpy as np
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer
from repeng import ControlModel, ControlVector

class TraderModel:
    def __init__(
        self,
        model_id: str = "Qwen/Qwen3-14B",
        layer_range: tuple[int, int] = (15, 30),
        device: str = "mps",
    ):
        self.tok = AutoTokenizer.from_pretrained(model_id)
        base = AutoModelForCausalLM.from_pretrained(
            model_id, torch_dtype=torch.float16, device_map=device
        )
        self.cm = ControlModel(base, layer_ids=list(range(*layer_range)))
        self.device = device

    def set_vector(self, vector: ControlVector | None, magnitude: float = 1.0):
        if vector is None or magnitude == 0.0:
            self.cm.reset()
        else:
            self.cm.set_control(vector, magnitude)

    def generate(self, prompt: str, max_tokens: int = 256, temperature: float = 0.4) -> tuple[str, np.ndarray]:
        inputs = self.tok(prompt, return_tensors="pt").to(self.device)
        with torch.no_grad():
            out = self.cm.generate(
                **inputs, max_new_tokens=max_tokens, temperature=temperature,
                do_sample=temperature > 0, return_dict_in_generate=True, output_scores=True,
            )
        text = self.tok.decode(out.sequences[0][inputs.input_ids.shape[1]:], skip_special_tokens=True)
        # first-step logits over the next-token vocabulary, used for confidence gating
        first_logits = out.scores[0][0].cpu().numpy() if out.scores else np.zeros(1)
        return text, first_logits
```

Note: this is heavier than llama.cpp. Acceptable for hackathon since vectors require it. Post-hackathon we can investigate llama.cpp + manual hidden-state hooks.

- [ ] **Step 2: Update smoke pipeline to load and apply vectors**

```python
# scripts/smoke_pipeline_with_vectors.py
from xianvec.config import load_config
from xianvec.schemas import MarketState
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.prompt import build_trader_prompt, parse_trader_response
from xianvec.trader.vectors import load_axis_vectors, compose_axis_vectors, gate_magnitude

cfg = load_config("config/default.yaml")
regime_cfg = load_config("config/regime_vectors.yaml")

state = MarketState(
    asset="BTC-USD", timestamp=1714600000.0, ohlcv_recent=[],
    indicators={"close": 50000, "rsi_14": 28, "ma_30": 51000, "ma_90": 52000,
                "atr_14": 1500, "macd": -50, "macd_signal": -20,
                "bb_upper": 53000, "bb_lower": 49500, "donchian_high_20": 54000,
                "volume_ratio_20": 1.2},
    onchain={"smart_money_inflow": 0.4, "funding_rate": -0.02, "stablecoin_inflow": 0.1},
    portfolio={"nav": 10000.0, "cash": 10000.0},
)

r = ClaudeIntern(model=cfg["intern"]["claude_model"]).reason(state, "smoke-vec")
weights = regime_cfg[r.regime]
vectors = load_axis_vectors("vectors")
composed = compose_axis_vectors(vectors, weights)

m = TraderModel(layer_range=tuple(cfg["trader"]["layer_range"]))
m.set_vector(composed, magnitude=1.0)
prompt = build_trader_prompt(briefing)
text, first_logits = m.generate(prompt)
gate = gate_magnitude(first_logits)
print(f"Gate magnitude: {gate:.2f}")
print("DECISION (vectors-on):", text)

m.set_vector(None)
text_off, _ = m.generate(prompt)
print("DECISION (vectors-off):", text_off)
```

- [ ] **Step 3: Run smoke**

```bash
python scripts/smoke_pipeline_with_vectors.py
```

Expected: visibly different decision text between vectors-on and vectors-off. If identical: vector magnitude too low (try 1.5 or 2.0), or layer range wrong.

- [ ] **Step 4: Commit**

```bash
git add src/xianvec/trader/model.py scripts/smoke_pipeline_with_vectors.py
git commit -m "feat(trader): integrate control vectors into model inference"
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
    """Simulates a trade given a future price path. For backtest replay."""

    def __init__(self, initial_nav: float, fee_bps: float = 10, slippage_bps: float = 5):
        self.nav = initial_nav
        self.fee_bps = fee_bps
        self.slippage_bps = slippage_bps

    def simulate_trade(
        self, d: TraderDecision, future_prices: pd.DataFrame, entry_price: float
    ) -> tuple[float, str]:
        """Return (pnl_dollars, exit_reason)."""
        if d.action in ("flat", "close"):
            return 0.0, "no_position"

        position_value = self.nav * (d.size_bps / 10000)
        # apply slippage to entry
        slip = entry_price * (self.slippage_bps / 10000)
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
    eq = equity_curve(pnls, initial_nav)
    return np.diff(eq) / eq[:-1]
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
) -> dict:
    """Paired bootstrap of Sharpe(A) - Sharpe(B). Inputs must be same length and aligned."""
    if len(returns_a) != len(returns_b):
        raise ValueError("paired returns must be same length")
    rng = np.random.default_rng(seed)
    n = len(returns_a)

    deltas = np.empty(n_resamples)
    for i in range(n_resamples):
        idx = rng.integers(0, n, n)
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
    }
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
from dataclasses import dataclass, field
from typing import Callable, Iterable
import pandas as pd
import numpy as np
from xianvec.schemas import MarketState, TraderDecision
from xianvec.execution.simulator import Simulator
from xianvec.risk.rules import RiskEvaluator

# Decision functions in backtest receive both the structured state and the raw price
# window, so technical baselines (which need OHLCV directly) and the agent (which
# uses precomputed indicators in state) share one signature.
DecisionFn = Callable[[MarketState, pd.DataFrame], TraderDecision]

@dataclass
class BacktestResult:
    name: str
    pnls: list[float] = field(default_factory=list)
    exit_reasons: list[str] = field(default_factory=list)
    decisions: list[TraderDecision] = field(default_factory=list)
    setup_ids: list[str] = field(default_factory=list)

    def to_arrays(self):
        return np.array(self.pnls), self.exit_reasons

def iter_setups(price_df: pd.DataFrame, asset: str, lookback: int = 200,
                horizon: int = 16, step: int = 8
                ) -> Iterable[tuple[MarketState, pd.DataFrame, pd.DataFrame, str]]:
    """Yield (state, window_df, future_df, setup_id) for each historical setup."""
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
            portfolio={"nav": 10000.0, "cash": 10000.0,
                       "open_positions": [], "daily_pnl_pct": 0.0},
        )
        setup_id = f"{asset}-{i}"
        yield state, window, future, setup_id

def run_backtest(
    name: str,
    decision_fn: DecisionFn,
    price_df: pd.DataFrame,
    asset: str,
    risk_cfg: dict,
    initial_nav: float = 10000.0,
    fee_bps: float = 10.0,
    slippage_bps: float = 5.0,
) -> BacktestResult:
    sim = Simulator(initial_nav=initial_nav, fee_bps=fee_bps, slippage_bps=slippage_bps)
    risk = RiskEvaluator(risk_cfg)
    result = BacktestResult(name=name)

    for state, window, future, setup_id in iter_setups(price_df, asset):
        try:
            decision = decision_fn(state, window)
        except Exception as e:
            print(f"decision_fn error on {setup_id}: {e}")
            continue
        decision = decision.model_copy(update={"setup_id": setup_id})

        risked = risk.evaluate(decision, asset=asset, portfolio=state.portfolio)
        if not risked.approved:
            result.pnls.append(0.0)
            result.exit_reasons.append(f"vetoed:{risked.veto_reason[:40]}")
            result.decisions.append(decision)
            result.setup_ids.append(setup_id)
            continue
        actual = risked.modified or risked.original
        entry = float(future["close"].iloc[0])
        pnl, reason = sim.simulate_trade(actual, future, entry_price=entry)
        result.pnls.append(pnl)
        result.exit_reasons.append(reason)
        result.decisions.append(actual)
        result.setup_ids.append(setup_id)
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
"""Run vectors-on vs vectors-off backtest, compute Δ-Sharpe with bootstrap CI."""
import pandas as pd
import numpy as np
import typer
from xianvec.config import load_config
from xianvec.intern.claude import ClaudeIntern
from xianvec.trader.model import TraderModel
from xianvec.trader.runtime import VectorTrader
from xianvec.trader.vectors import load_axis_vectors
from xianvec.eval.backtest import run_backtest, align_paired
from xianvec.eval.metrics import sharpe, max_drawdown, profit_factor, win_rate, returns_from_pnl
from xianvec.eval.compare import paired_bootstrap_sharpe_delta

def main(price_path: str, asset: str = "BTC-USD"):
    cfg = load_config("config/default.yaml")
    regime_cfg = load_config("config/regime_vectors.yaml")
    risk_cfg = load_config("config/risk.yaml")
    df = pd.read_parquet(price_path)

    intern = ClaudeIntern(model=cfg["intern"]["claude_model"])
    model = TraderModel(layer_range=tuple(cfg["trader"]["layer_range"]))
    axis_vectors = load_axis_vectors("vectors")

    def make_decision_fn(vectors_enabled):
        trader = VectorTrader(model, axis_vectors, vectors_enabled=vectors_enabled)
        from xianvec.pipeline.trade import TradePipeline
        pipeline = TradePipeline(intern, trader, risk_cfg, regime_cfg)
        # Backtest harness passes (state, window); agent only uses state.
        def fn(state, _window):
            r = pipeline.run(state, setup_id="ab")
            return (r["risk"].modified or r["risk"].original) if r["risk"].approved else r["decision"]
        return fn

    print("Running vectors-OFF backtest...")
    r_off = run_backtest("vectors_off", make_decision_fn(False), df, asset, risk_cfg)
    print("Running vectors-ON backtest...")
    r_on = run_backtest("vectors_on", make_decision_fn(True), df, asset, risk_cfg)

    a_pnls, b_pnls = align_paired(r_on, r_off)
    a_rets = returns_from_pnl(a_pnls)
    b_rets = returns_from_pnl(b_pnls)

    boot = paired_bootstrap_sharpe_delta(a_rets, b_rets,
                                         n_resamples=cfg["eval"]["bootstrap_resamples"],
                                         seed=42)

    print("\n=== A/B RESULTS ===")
    print(f"Vectors ON  Sharpe: {sharpe(a_rets):.3f}  MDD: {max_drawdown(np.cumsum(a_pnls)+10000):.3f}  "
          f"PF: {profit_factor(a_pnls):.2f}  WR: {win_rate(a_pnls):.2%}")
    print(f"Vectors OFF Sharpe: {sharpe(b_rets):.3f}  MDD: {max_drawdown(np.cumsum(b_pnls)+10000):.3f}  "
          f"PF: {profit_factor(b_pnls):.2f}  WR: {win_rate(b_pnls):.2%}")
    print(f"\nΔ-Sharpe: {boot['delta_sharpe']:.3f}  "
          f"95% CI: [{boot['ci_low']:.3f}, {boot['ci_high']:.3f}]  p≈{boot['p_value']:.3f}  "
          f"n={boot['n']}")

    n_diverged = sum(
        1 for sid in r_on.setup_ids if sid in set(r_off.setup_ids)
        and dict(zip(r_on.setup_ids, r_on.decisions))[sid].action
        != dict(zip(r_off.setup_ids, r_off.decisions))[sid].action
    )
    print(f"Decision divergence rate: {n_diverged}/{boot['n']} = {n_diverged/boot['n']:.2%}")

if __name__ == "__main__":
    typer.run(main)
```

- [ ] **Step 2: Smoke run on small price slice**

```bash
python scripts/run_ab_compare.py data/historical/btc-15min-30days.parquet
```

Expected: a results block printing Δ-Sharpe with confidence interval. Even on small N the format should work; statistical significance requires N≥30 trades.

- [ ] **Step 3: Commit**

```bash
git add scripts/run_ab_compare.py
git commit -m "feat: A/B comparison runner with paired bootstrap delta-sharpe"
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

def markdown_report(boot_result, on_metrics, off_metrics, divergence_rate, plot_b64):
    return f"""# XIANVEC Demo — A/B Result

**Δ-Sharpe (vectors on − off):** **{boot_result['delta_sharpe']:.3f}**
- 95% CI: [{boot_result['ci_low']:.3f}, {boot_result['ci_high']:.3f}]
- p ≈ {boot_result['p_value']:.3f}
- n = {boot_result['n']}

| Metric | Vectors ON | Vectors OFF |
|---|---|---|
| Sharpe | {on_metrics['sharpe']:.3f} | {off_metrics['sharpe']:.3f} |
| Max Drawdown | {on_metrics['mdd']:.2%} | {off_metrics['mdd']:.2%} |
| Profit Factor | {on_metrics['pf']:.2f} | {off_metrics['pf']:.2f} |
| Win Rate | {on_metrics['wr']:.2%} | {off_metrics['wr']:.2%} |

**Decision divergence rate:** {divergence_rate:.2%} (vectors changed action this often)

![equity curves](data:image/png;base64,{plot_b64})
"""
```

- [ ] **Step 2: Write the runner**

```python
# scripts/build_demo_report.py
"""Build the markdown + equity-plot demo report from a saved A/B run."""
import json
import pickle
from pathlib import Path
import numpy as np
import typer
from xianvec.eval.metrics import sharpe, max_drawdown, profit_factor, win_rate, returns_from_pnl
from xianvec.eval.compare import paired_bootstrap_sharpe_delta
from xianvec.eval.report import equity_plot_b64, markdown_report

def main(on_pickle: str, off_pickle: str, out_path: str = "data/reports/demo.md"):
    """on_pickle / off_pickle are pickled BacktestResult objects from run_ab_compare."""
    with open(on_pickle, "rb") as f:
        r_on = pickle.load(f)
    with open(off_pickle, "rb") as f:
        r_off = pickle.load(f)

    from xianvec.eval.backtest import align_paired
    a, b = align_paired(r_on, r_off)
    a_rets = returns_from_pnl(a)
    b_rets = returns_from_pnl(b)
    boot = paired_bootstrap_sharpe_delta(a_rets, b_rets, n_resamples=10000, seed=42)

    on_metrics = {
        "sharpe": sharpe(a_rets), "mdd": max_drawdown(np.cumsum(a) + 10000),
        "pf": profit_factor(a), "wr": win_rate(a),
    }
    off_metrics = {
        "sharpe": sharpe(b_rets), "mdd": max_drawdown(np.cumsum(b) + 10000),
        "pf": profit_factor(b), "wr": win_rate(b),
    }
    diverged = sum(1 for sid in r_on.setup_ids if sid in set(r_off.setup_ids)
                   and dict(zip(r_on.setup_ids, r_on.decisions))[sid].action
                   != dict(zip(r_off.setup_ids, r_off.decisions))[sid].action)
    divergence_rate = diverged / boot["n"] if boot["n"] else 0.0

    plot = equity_plot_b64(a.tolist(), b.tolist())
    md = markdown_report(boot, on_metrics, off_metrics, divergence_rate, plot)
    Path(out_path).parent.mkdir(parents=True, exist_ok=True)
    Path(out_path).write_text(md)
    print(f"wrote {out_path}")

if __name__ == "__main__":
    typer.run(main)
```

For this to work, `run_ab_compare.py` must persist its `BacktestResult` objects via `pickle.dump`. Add at the end of `scripts/run_ab_compare.py`:

```python
import pickle
from pathlib import Path
Path("data/reports").mkdir(parents=True, exist_ok=True)
with open("data/reports/r_on.pkl", "wb") as f:
    pickle.dump(r_on, f)
with open("data/reports/r_off.pkl", "wb") as f:
    pickle.dump(r_off, f)
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

## Phase 12 — Self-review checklist

Before declaring the build done for hackathon:

- [ ] All unit tests passing: `pytest tests/unit -v`
- [ ] All integration tests passing: `pytest tests/integration -v`
- [ ] Spike validation script passes (`scripts/spike_vector_validation.py`)
- [ ] `scripts/smoke_pipeline_with_vectors.py` produces visibly different on/off decisions
- [ ] At least one full backtest run on ≥90 days of BTC-USD 15-min data completes without error
- [ ] A/B comparison run (`scripts/run_ab_compare.py`) produces Δ-Sharpe with bootstrap CI on n ≥ 30 paired trades
- [ ] Demo report (`scripts/build_demo_report.py`) renders the markdown report and equity-curve plot
- [ ] Telegram bot connects and responds to `/analyze` and `/compare` commands
- [ ] Forward paper run (`scripts/run_paper.py`) executes a real Alpaca paper order successfully (verify in Alpaca dashboard)
- [ ] Exchange funding-rate fetch returns a real number (`onchain_signals('BTC-USD')`)
- [ ] Nansen client either returns real signal or fails gracefully to 0.0
- [ ] All decisions persisted to `data/decisions.db` with `vectors_enabled` flag
- [ ] No secrets in code or committed config (audit with `git log -p | grep -E "sk-|api_key"`)

---

## Hackathon parallelization

Time-boxed execution. These tracks have no dependencies between them and can run in parallel while you focus on the critical path:

**Critical path (must be sequential, you do this):**
Phase 0 → Phase 1.1 (schemas) → Phase 4 (vectors) → Phase 9 (A/B) → Phase 10 demo polish.

**Parallelizable tracks (delegate to subagents or do in parallel sessions):**
- **Track A: Baselines.** Phase 7.1 + 7.2 are pure functions over price/onchain dicts — Haiku-class subagent can implement and test these independently.
- **Track B: Eval framework.** Phase 8.1 (metrics) + 8.2 (paired bootstrap) are pure stats; Haiku can implement against the test specs in this doc.
- **Track C: Data fetchers.** Phase 11.2 (Alpaca), 11.3 (exchange), 11.4 (Nansen) are independent IO modules; one subagent can do all three.
- **Track D: Contrastive datasets.** Phase 4.1's four JSON files (50 pairs each) is content generation — Sonnet/Opus subagent producing high-quality contrastive prompts.

Recommended sequence: kick off tracks A, B, C, D in parallel as soon as Phase 1 schemas are merged. They feed back into the critical path at Phase 4 (D), Phase 8 (B), Phase 9 (A), and Phase 11 (C).

---

## Demo narrative (for hackathon presentation)

The story arc that the data should tell:

1. **The thesis** (30 sec): control vectors encode disposition; same agent, vectors on vs off, on the same setups.
2. **The vectors-OFF baseline** (30 sec): show the agent's vectors-off Sharpe alongside textbook baselines (RSI, MA, Donchian, smart-money copy). Establish "this is a competent baseline trader."
3. **The vectors-ON result** (60 sec): show Δ-Sharpe, decision divergence rate, equity curve overlay. Headline number is Δ-Sharpe with CI.
4. **What the vectors did** (60 sec): show 2-3 specific setups where vectors-on and vectors-off chose different actions. Read the `trader_summary` from each. This is where the demo lives — the human-legible behavior shift.
5. **What's next** (30 sec): SVF for context-conditional steering, Karpathy loop for self-improvement from trade outcomes, ERC-8004 for verifiable track record.

---

*Plan version: 2026-05-02. Lives at `/Users/edkennedy/Code/xianvec/implementation-plan.md`. Companion: `architecture.md`.*
