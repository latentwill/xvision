# Nansen + Elfa forward-only data tools

- **Date:** 2026-06-14
- **Status:** Draft (design locked via grilling session; pending Plan Review Gate before implementation)
- **Owner:** Edward
- **Related:** `crates/xvision-engine/src/api/eval.rs` (Cline tool bridge), `crates/xvision-engine/src/eval/` (RunMode + shared executor), `crates/xvision-engine/src/agent/cline_recording.rs` (trajectory recording), `crates/xvision-data/src/alpaca.rs` (HTTP client template), `crates/xvision-core/src/asset_registry.rs` (asset identity)

## 1. Goal

Add two external data providers as **trader-callable tools** so the LLM trader can pull
decision-relevant signals:

- **Nansen** — on-chain analytics (smart-money flows, token screener, token-god-mode flow intelligence).
- **Elfa** — crypto social / KOL intelligence (smart mentions, trending tokens, trending narratives).

Hard constraint: **Elfa is forward-only** (forbidden in backtest). **Nansen is usable in both
modes**, but in backtest it must use Nansen's lookahead-safe point-in-time API anchored to the
simulated clock — never live/current-state data. The enforcement of these rules must be
**explicit and testable**, not incidental.

## 2. Key architectural findings (verified in code)

These overturn the naive "implement a `Tool`, it's forward-only for free" assumption and shape
the whole design.

1. **The trader runs on the Cline sidecar, not `execute_slot`/`LlmDispatch`.**
   `crates/xvision-engine/src/agent/dispatch_capability.rs:35` —
   `// SlotInput and execute_slot were removed in WU-6 (LlmDispatch trader retirement)`.
   The trader dispatches via `execute_slot_cline` → the `xvision-agentd` sidecar.

2. **Tools still live in `Tool` + `ToolRegistry`, bridged to the sidecar.** The sidecar runs the
   trader's tool-use loop and calls *back* into Rust over a Unix socket. The bridge is
   `ToolRegistryDispatch` (`crates/xvision-engine/src/api/eval.rs:2293`), whose doc comment says it
   *"Bridges sidecar tool callbacks to the engine's `ToolRegistry`… so both runtimes share one tool surface."*
   Chain: `impl ToolDispatch::invoke` (`:2328`) → `tool_call::invoke(name, input, self.tools)` →
   `ToolRegistry.get(name).invoke()` → `Tool::invoke`. Built at `:2428`, passed as `dispatch` into
   `AgentClient::spawn_with_event_sink(…)` (`:2443`) → `serve_callbacks(cb_sock, dispatch)`
   (`crates/xvision-agent-client/src/client.rs:166`).

3. **`ToolRegistryDispatch::invoke` is the single chokepoint for every trader tool call**, and it
   already (a) enforces per-call policy — `callback_market_data_tool_asset_mismatch` rejects
   cross-asset fetches (`api/eval.rs:2304`), and (b) holds per-cycle mutable context —
   `current_asset: Arc<RwLock<Option<String>>>` (`:2295`), fed from a handle created at `:2427`,
   shared into the dispatch at `:2430` and into `ClineDispatchCtx` at `:2506`
   (`dispatch_capability.rs:80`). **This is where our gate and `as_of` injection go — no `Tool`
   trait change required.**

4. **`RunMode { Backtest, Live }` already exists** (`crates/xvision-core/src/config.rs` and
   `crates/xvision-engine/src/eval/run.rs:186`; note `"paper"` parses to `Backtest`) but is **not**
   threaded into the agent/tool layer today.

5. **Backtest already runs the LLM pipeline.** A single `Executor`
   (`crates/xvision-engine/src/eval/executor/mod.rs:5`) covers both `RunMode::Backtest` and `Live`
   by swapping `BarSource` + `Clock`; the trader runs through Cline in both. So a tool registered
   today is reachable in backtest **right now** — the forward-only rule is a live correctness gap,
   not a hypothetical.

6. **Trajectory recording captures LLM frames only** (`agent/cline_recording.rs`). External tool
   HTTP responses are **not** recorded, so backtest re-runs would re-fetch (cost + nondeterminism).

7. **No asset→on-chain identity mapping exists.** `RegistryEntry`
   (`crates/xvision-core/src/asset_registry.rs`) carries `orderly_symbol` / `alpaca_pair` /
   `category` but **no chain or contract address**; `AssetSymbol`
   (`crates/xvision-core/src/trading.rs`) is a bare ticker. Nansen needs chain+contract; Elfa needs `$TICKER`.

8. **`ProviderKind` is LLM-only** (`config.rs:71`: Anthropic, OpenaiCompat, LocalCandle, Ollama,
   LlamaCpp, Vllm). Data/signal tools need their own config type, not an overloaded LLM provider.

## 3. Decisions (locked)

| # | Decision | Choice | Rationale |
|---|---|---|---|
| D1 | Backtest policy | **Elfa forbidden in backtest; Nansen `/v1beta1` historical allowed in backtest** | Nansen ships a lookahead-safe point-in-time API; Elfa has no point-in-time mode. |
| D2 | Backtest must actually run the tools | **Full — Nansen-historical runs end-to-end in backtest** | "Gotta be able to use it." The LLM-in-backtest path already exists (finding #5). |
| D3 | Determinism + cost of backtest re-runs | **Record tool responses + replay** (extend trajectory store) | Reproducible, free on re-run, frozen against vendor label revisions; makes the lookahead test assertable. |
| D4 | `as_of_date` granularity / lookahead lag | **Strict — last fully-completed UTC day** (`floor_to_day(T) − lag`, `nansen_lookahead_lag_days` default 1) | `as_of_date` is day-granular; same-day data leaks post-decision flows. Computed by the framework from the simulated clock; the model cannot override it. |
| D5 | v1 scope | **Rich — 6 tools** | 3 Nansen (live+historical) + 3 Elfa (forward-only). |
| D6 | Asset identity | **Extend `RegistryEntry`** with `chain` + `contract_address`; derive `$TICKER` from symbol | One typed source of truth. |
| D7 | Auth | **API key via env indirection** (`NANSEN_API_KEY` `apikey` header; `ELFA_API_KEY` `x-elfa-api-key`), managed in a new **Settings → Tools** section | Mirrors Alpaca; keys stay in env/`.op_env`, config references the env var name only. x402 explicitly **out** (see §11). |
| D8 | Failure mode | **Degrade + per-run credit budget cap** | Tool returns `{available:false, reason}`; trader proceeds; no cycle ever blocks on an external signal. |

Defaults also locked: single capability name per signal (dispatch auto-routes live vs historical
by `run_mode`); tools added to intern/research + trader starter templates, opt-in per strategy via
`allowed_tools`; observability reuses trajectory recording + an inline signal-chip row on cycle
detail (no side panel / popup, per house rules); TDD with the coverage gate.

## 4. Tool surface

Six capability names. Each is one tool the trader sees; the dispatch picks the endpoint by `run_mode`.

| Tool name | Live endpoint (`/api/v1`) | Backtest endpoint (`/api/v1beta1`) | Modes |
|---|---|---|---|
| `nansen_smart_money_flow` | `smart-money/netflow` | `smart-money/historical-token-balances` / `tgm/historical-token-flow-summary` | Live + Backtest |
| `nansen_token_screener` | `tgm/token-screener` | `token-screener/historical` | Live + Backtest |
| `nansen_flow_intel` | `tgm/flow-intelligence` | `tgm/historical-who-bought-sold` + `tgm/historical-token-quant-scores` | Live + Backtest |
| `elfa_smart_mentions` | `v2/data/top-mentions` | — | Live only |
| `elfa_trending_tokens` | `v2/aggregations/trending-tokens` | — | Live only |
| `elfa_trending_narratives` | `v2/data/trending-narratives` | — | Live only |

Naming is operator-facing (Settings + `allowed_tools`); keep it plain-language and stable.

## 5. Design

### 5.1 Forward-only enforcement (the spine)

A per-tool **mode policy** table in the engine (no external-crate change, since `ToolDescriptor`
lives in `xvision_agent_client::protocol`):

```rust
// crates/xvision-engine/src/tools/signal_policy.rs (new)
pub struct ToolModePolicy {
    pub live: bool,                 // advertised + callable in RunMode::Live
    pub backtest: Option<Backtest>, // Some => callable in Backtest via historical binding
}
pub struct Backtest { /* marker: tool routes to its v1beta1 binding with injected as_of */ }

pub fn signal_tool_policy(name: &ToolName) -> Option<&'static ToolModePolicy>; // None => unrestricted built-in
```

Enforcement is **three layers**:

1. **Advertisement filter.** Where `ClineSlotInput.allowed_tools` is assembled (the list sent to
   the sidecar in `StartRunParams.allowed_tools`, `agent/execute_cline.rs:385`), strip any
   forward-only tool whose policy has no binding for the active `run_mode`. In `Backtest`, Elfa
   names are removed before the trader ever sees them.
2. **Dispatch chokepoint guard (defense in depth).** Extend `ToolRegistryDispatch` with
   `run_mode: RunMode` (set at construction, `api/eval.rs:2428`) and consult `signal_tool_policy`
   in `invoke` (`:2328`), returning `ToolDispatchError::Failed("…forward-only; unavailable in backtest")`
   if a forbidden tool is somehow called. This mirrors the existing asset-mismatch guard exactly.
3. **`as_of` injection for Nansen-historical.** Add
   `as_of: Arc<RwLock<Option<DateTime<Utc>>>>` to `ToolRegistryDispatch`, created and shared
   exactly like `tool_asset_guard` (`api/eval.rs:2427` → `:2430` → `ClineDispatchCtx`, `:2506`).
   The executor writes the cycle's simulated timestamp into it each decision (alongside the
   existing `current_asset` write). In `invoke`, for a Nansen tool under `RunMode::Backtest`,
   compute `as_of_date = floor_to_utc_day(as_of) − nansen_lookahead_lag_days` and **inject it into
   the tool input**, overwriting any model-supplied value. The model cannot influence the
   backtest anchor — this is the lookahead-safety invariant.

### 5.2 Determinism + replay (D3)

Extend the trajectory recording to capture tool HTTP responses:

- On a **recording** run, after `ToolRegistryDispatch::invoke` resolves a Nansen/Elfa response,
  persist `(recording_id, tool_name, input_hash, as_of) → response_json` into the trajectory
  store (sibling to the existing frame tables in `cline_recording.rs`).
- On a **replay** run (backtest re-run), `invoke` serves from the store and makes **no HTTP call**.
- Live/forward runs never replay; they always fetch.

Result: identical backtest re-runs are free and byte-identical, and historical Nansen data is
frozen at record time.

### 5.3 Asset identity (D6)

```rust
// crates/xvision-core/src/asset_registry.rs — RegistryEntry, add:
pub chain: Option<String>,            // "ethereum" | "solana" | "base" | …  (Nansen)
pub contract_address: Option<String>, // token contract / mint            (Nansen)
```

Elfa `$TICKER` is derived from `AssetSymbol::as_str()`. Seed `chain`/`contract_address` for the
enabled crypto whitelist. Unmapped assets degrade per D8 (tool returns `{available:false,
reason:"no on-chain identity mapped for X"}`), never panic.

### 5.4 HTTP clients

New `crates/xvision-data/src/nansen.rs` and `elfa.rs`, modeled on `alpaca.rs`:
`reqwest::Client::builder().timeout(...)`, a `governor` rate limiter
(`Quota::per_minute` — Nansen 300/min, Elfa 60/min defaults), a typed error enum, and key loading
via the `resolve_api_key` env-indirection convention (`crates/xvision-engine/src/providers/fetcher.rs`).
Nansen = `POST` + JSON body, header `apikey`; Elfa = `GET` + query params, header `x-elfa-api-key`.

### 5.5 Config + Settings UI (D7)

A dedicated data-tools config type (not `ProviderEntry`):

```rust
// crates/xvision-core/src/config.rs (new)
pub enum DataToolKind { Nansen, Elfa }
pub struct DataToolEntry {
    pub kind: DataToolKind,
    pub base_url: String,
    pub api_key_env: String,     // env var NAME, never the secret
    pub enabled: bool,
    pub budget_credits_per_run: Option<u32>, // D8 cap
}
```

Frontend: add `frontend/web/src/routes/settings/tools.tsx` alongside `settings/providers.tsx`,
register in `routes.tsx`, link from `settings/index.tsx`. Backend CRUD mirrors the providers
settings endpoint. It's a route, not a popup/side-panel — compliant with the house UI rules.

### 5.6 Failure mode + budget (D8)

`ToolRegistryDispatch::invoke` wraps each fetch: on transport error, 429, 4xx-credits, or unmapped
asset, return a structured `{available:false, reason}` value (a successful tool result, not an
`Err`, so the Cline loop continues). A per-run credit counter (seeded from
`budget_credits_per_run`) disables the tool for the rest of the run once exhausted, returning
`{available:false, reason:"budget exhausted"}`.

### 5.7 Observability

Tool calls already surface via the trajectory/trace recording. Add an inline "signals used" chip
row on the cycle-detail page (`frontend/web/src/routes/strategies-detail.tsx` / cycle detail),
rendered as a full-width horizontal strip — no fourth column, no popup.

## 6. File-by-file change map

- `crates/xvision-data/src/nansen.rs`, `elfa.rs` — **new** HTTP clients.
- `crates/xvision-engine/src/tools/nansen_*.rs`, `elfa_*.rs` — **new** `Tool` impls (6).
- `crates/xvision-engine/src/tools/signal_policy.rs` — **new** mode-policy table.
- `crates/xvision-engine/src/tools/mod.rs` — register the 6 tools in the registry.
- `crates/xvision-engine/src/api/eval.rs` — add `run_mode` + `as_of` to `ToolRegistryDispatch`;
  guard + inject in `invoke`; create/share the `as_of` handle (mirror `:2427`/`:2430`/`:2506`).
- `crates/xvision-engine/src/agent/dispatch_capability.rs` — add `as_of` field to `ClineDispatchCtx`;
  per-decision write alongside `tool_asset_guard`.
- `crates/xvision-engine/src/agent/execute_cline.rs` — advertisement filter on `allowed_tools` by `run_mode`.
- `crates/xvision-engine/src/agent/cline_recording.rs` — record/replay tool HTTP responses.
- `crates/xvision-core/src/asset_registry.rs` — add `chain` + `contract_address`; seed whitelist.
- `crates/xvision-core/src/config.rs` — `DataToolEntry` / `DataToolKind`.
- `crates/xvision-engine/src/agents/templates.rs` — add tool names to intern/trader starter templates.
- `frontend/web/src/routes/settings/tools.tsx` (+ `routes.tsx`, `settings/index.tsx`) — Tools settings.
- `.op_env` — `NANSEN_API_KEY`, `ELFA_API_KEY` (via 1Password).

## 7. Testing (TDD; meets the coverage gate)

Headline is the **lookahead-safety invariant**:

1. **Backtest gate.** A `RunMode::Backtest` dispatch refuses every Elfa tool (`ToolDispatchError`)
   and never advertises them in `allowed_tools`.
2. **`as_of` flooring.** Given a simulated decision at `2024-03-15T14:00Z` with `lag=1`, the Nansen
   tool is invoked with `as_of_date=2024-03-14`, regardless of any model-supplied date.
3. **Deterministic replay.** A recorded backtest re-run produces byte-identical tool outputs and
   makes zero HTTP calls.
4. **Routing.** The same `nansen_smart_money_flow` name hits `/v1` under `Live` and `/v1beta1`
   under `Backtest`.
5. **Degrade.** Simulated 429 / unmapped asset / budget-exhausted each yield `{available:false}`
   and the cycle still completes.
6. **HTTP parsing.** Fixture-based unit tests for each client against captured JSON.

## 8. Risks / open questions

- **Live LLM-in-backtest cost.** Recording a fresh backtest still makes real LLM + Nansen calls
  once; only re-runs are free. Mitigate with the budget cap and small bar ranges while iterating.
- **Nansen `/v1beta1` coverage per metric.** Confirm each of the 3 Nansen tools has a usable
  historical counterpart and matching response shape before committing the v1beta1 binding;
  fall back to "backtest-unavailable" for any metric that lacks one.
- **`as_of_date` semantics.** Verify against live Nansen docs that `as_of_date` is day-granular and
  that snapshot vs window endpoints take the params assumed here.
- **Per-decision `as_of` writer.** Confirm the exact executor site that currently writes
  `current_asset` so the `as_of` write lands in the same place.

## 9. Implementation sequence

1. `RunMode` + `as_of` threading into `ToolRegistryDispatch`; mode-policy table; advertisement
   filter; guard. (Pure enforcement scaffold + tests 1–2,4 — no network.)
2. Nansen client + the 3 Nansen tools (live `/v1`), API key config, Settings → Tools section.
3. Nansen `/v1beta1` historical binding + `as_of` injection + record/replay (tests 3).
4. Elfa client + the 3 Elfa tools (live only), forbidden-in-backtest tests.
5. Asset-registry identity fields + whitelist seeding; degrade/budget paths (tests 5).
6. Starter-template wiring; inline signal chips; docs (MANUAL.md / dashboard wiki).

## 10. Out of scope (future)

- **x402 pay-per-call** (USDC on Base via xvision-identity). Both APIs support it; deferred in
  favor of plain API keys in Settings.
- **Nansen Smart Alerts** (dashboard-only; would require polling).
- **Portfolio-mode** multi-asset signal fan-out.
