# LLM Providers & Per-Arm Models — Design

> **Status:** Draft for user review · 2026-05-10
> **Author:** xvision hackathon team
> **Companion specs:** [Strategy Creation Engine](./2026-05-08-strategy-creation-engine-design.md) (drafts that this surfaces in `/strategies`) · [Slot Machine](./2026-05-08-slot-machine-design.md) (per-variant model sampling consumer of this surface)
> **Hackathon deadline:** 2026-06-15

---

## 1. Purpose, scope, and persona

Today, `xvn ab-compare` accepts one Intern provider/model and one Trader provider/model and applies them to every `trader_arm` instance. The product question this fails to answer is **"does strategy X work better with Claude, GPT-4o, Llama, or Qwen?"** — the most natural A/B a strategy author wants to run, and the one Persona A reaches for first.

This spec defines:

1. A **registry of LLM providers** in user settings — one `(name, kind, base_url, api_key_env)` row per provider, defined once and referenced everywhere.
2. **Per-slot model selection** at the arm level — Intern model and Trader model become independently selectable on each `trader_arm` instance, with provider implicit from the model's registered provider OR explicit on the slot.
3. **Per-arm model override in the `xvn ab-compare` arm spec** so the same fork shows up at the CLI, not just the UI.
4. **A `Fork with different model →` action** on `/strategies` rows that pre-fills the Inspector with a duplicated draft and the Provider+Model select pre-focused on one slot.
5. **BriefingCache cost cues** in the UI so users know that swapping the Trader model is cheap (Stage 1 reused) but swapping the Intern model is full-pipeline expensive.

**Personas:**
- **Persona A** (strategy author) — opens Inspector, sees three LLM slots (Regime / Intern / Trader), each with Provider + Model dropdowns sourced from `/settings`. Forks a strategy from the list and changes one model; runs eval; compares.
- **Persona B** (CLI user / hackathon judge) — runs `xvn ab-compare --arms 'trader_arm:trader=anthropic/claude-opus-4-7,trader_arm:trader=openai/gpt-4o,trader_arm:trader=together/llama-3.3-70b' …` and gets a single `BacktestResult` JSON with three independent arm result rows.

### 1.1 In scope (v1, by 2026-06-15)

- New `[[providers]]` array in `config/default.toml` with `{name, kind, base_url, api_key_env}` rows.
- `RuntimeConfig.providers: Vec<ProviderEntry>`; existing `[intern]` block keeps working but rewritten to be a slot-shaped reference (`provider = "<name>"`, `model = "..."`) with a one-shot migration helper.
- `ArmKind::Trader` extended from unit variant to struct with `intern: Option<SlotRef>`, `trader: Option<SlotRef>` where `SlotRef = { provider: String, model: String }`. `None` means "fall back to the CLI default" (existing behavior).
- `parse_arm_spec` extended to accept `trader_arm:intern=<provider>/<model>:trader=<provider>/<model>` (also `intern_model=…:trader_model=…` shorthand when provider is the global default).
- `run_ab_compare` builds per-arm Intern + Trader backends from the SlotRef when present, otherwise clones the shared global Arc (existing behavior preserved).
- BriefingCache key already keys on `(cycle_id, intern_provider, intern_model)` — no schema change; just verify divergence semantics with a test.
- `/settings → 13.1 LLM keys` UI section becomes `13.1 Providers`: list view + add/edit/delete row affordances. (UI design lock, not implementation — there is no built UI yet.)
- Inspector LLM slot section gains a `Provider` select alongside the existing `Model` select (UI design lock).
- `/strategies` row `⋯` action menu gains `Fork with different model →` (UI design lock).
- A markdown migration note for existing `config/default.toml` users.

### 1.2 Out of scope (v1; deferred)

- Built UI — only the design lock; the `.tsx` build is a separate effort tracked in the strategy-engine 2D-dashboard plan.
- Provider-side feature detection (which models support `reasoning_effort`, vision, etc.). v1 lets the user paste any model id and trusts the backend.
- Per-slot **prompt** versioning (covered by the Inspector live-preview / prompt diff design in `ui-elements.md` §4.2.2).
- Cost telemetry rollups (token spend per arm). Tracked via existing trace `attrs_json` (`gen_ai.usage.*`); aggregation is downstream.
- Model leaderboards across the fleet of registered providers (a marketplace-tab future).
- Local-candle slot wiring — existing `LocalCandle` provider variant continues to be a no-op stub; not enabled by this spec.

### 1.3 Backwards compatibility

| Existing surface | After v1 |
|---|---|
| `config/default.toml [intern] {provider=..., base_url=..., model=..., api_key_env=...}` | Keeps loading. A new auto-derived `provider` row is added to the in-memory `providers` vec on load if its `(kind, base_url, api_key_env)` triple isn't already named. The `[intern]` block becomes a "default Intern slot" alias for the autoderived row. |
| `xvn ab-compare --intern --intern-model --trader-base-url --trader-model --trader-api-key-env` | Keeps working. These flags become the **default** Intern + Trader for any `trader_arm` that doesn't carry an inline override in the arm spec. |
| `parse_arm_spec("trader_arm")` | Returns `ArmKind::Trader { intern: None, trader: None }` — same effective behavior as today. |
| `run_ab_compare` signature | Extended; existing call sites pass `None` for the new optional resolver and get today's behavior. |

No file is renamed, no field is removed. All migration is additive.

---

## 2. Locked decisions

| # | Decision |
|---|---|
| 1 | **Providers are first-class config rows.** A `[[providers]]` array in `config/default.toml`, each row referenced by `name`. Slots reference a provider by name + supply a `model` string. Same shape as how the strategies folder treats `Strategy` impls — registered once, referenced many times. |
| 2 | **Provider rows are config-only, not chain-anchored.** No ERC-8004 identity for providers themselves. (Per-arm `agent.json` already covers identity; the model swap is a property of the arm, not the provider.) |
| 3 | **Both Intern and Trader are slot-shaped.** Each slot is `{ provider_name, model }`. The `Regime` slot exists in the UI design but ships as a no-op until ADR for regime-classifier-as-LLM lands; the schema accommodates it. |
| 4 | **CLI flags are fallbacks; arm-spec inline wins.** Per-arm overrides in the arm spec take precedence over the global `--intern` / `--trader-*` flags, which take precedence over the `[intern]` block in `default.toml`. |
| 5 | **One backend instance per `(provider_name, model)` combo per run.** A small interner inside `run_ab_compare` deduplicates `OpenAiCompatBackend` / `AnthropicIntern` instances so two arms pointing at the same `(claude-opus-4-7)` share one HTTP client / connection pool. |
| 6 | **BriefingCache semantics are unchanged.** The existing key `(cycle_id, intern_provider, intern_model)` already means: same Intern config → shared briefing; different Intern config → independent briefings. v1 surfaces this in the UI as a hint chip but does not change the cache logic. |
| 7 | **The `[intern]` block stays.** It continues to define the workspace-default Intern slot. We do **not** force users to migrate to a `[[providers]]` + `[[slots]]` shape in v1; auto-derivation handles the bridge. (Re-evaluated post-hackathon if v2 grows multi-workspace.) |
| 8 | **API keys live in env, not in TOML.** `api_key_env` is the env-var **name**; the value is read at backend construction. This matches the current `Intern.api_key_env` field. The `/settings` UI never asks for or displays the secret — it asks for the env-var name and shows a `● set` / `○ missing` chip based on `std::env::var(name).is_ok()`. |

---

## 3. Architecture

### 3.1 Module layout

```
crates/xvision-core/src/config.rs
  + struct ProviderEntry { name, kind: ProviderKind, base_url, api_key_env }
  + enum ProviderKind { Anthropic, OpenaiCompat, LocalCandle }   // mirrors today's InternProvider
  + RuntimeConfig.providers: Vec<ProviderEntry>
  + RuntimeConfig::resolve_default_intern_slot(&self) -> SlotRef    // backed by [intern]
  + RuntimeConfig::auto_derive_intern_provider_row()                // mutates self on load if needed

crates/xvision-core/src/slot.rs    (NEW)
  + struct SlotRef { provider: String, model: String }
  + impl SlotRef { fn parse(&str) -> Result<Self, ParseError> }     // "<provider>/<model>"
  + impl Display for SlotRef                                        // "<provider>/<model>"

crates/xvision-eval/src/ab_compare.rs
  - ArmKind::Trader   (unit variant)
  + ArmKind::Trader { intern: Option<SlotRef>, trader: Option<SlotRef> }
  + parse_arm_spec    extended to read intern=…:trader=… kv pairs
  + run_ab_compare    accepts a ProviderRegistry; per-arm slot resolves into a backend Arc

crates/xvision-eval/src/provider_registry.rs   (NEW)
  + struct ProviderRegistry { rows: Vec<ProviderEntry>, default_intern: SlotRef, default_trader: SlotRef }
  + impl ProviderRegistry {
        fn intern_backend(&self, slot: &SlotRef) -> Result<Arc<dyn InternBackend>>
        fn trader_backend(&self, slot: &SlotRef) -> Result<Arc<dyn TraderBackend>>
        // both memoize on (provider_name, model)
    }

crates/xvision-cli/src/lib.rs
  + AbCompare gains no new flags  (existing flags become "defaults")
  + AbCompare emits a tracing line per arm:
       arm=<name> intern=<provider>/<model> trader=<provider>/<model>

crates/xvision-cli/src/commands/ab_compare.rs
  + builds ProviderRegistry from config + CLI flag fallbacks
  + threads registry into run_ab_compare

docs/design/ui-elements.md
  + §13.1 LLM keys → §13.1 Providers   (revised content; old field-level detail in v0.1 marked superseded)
  + §4.2.2 LLM slot sections — Provider select added alongside Model
  + §5 /strategies row action — `Fork with different model →` added
```

### 3.2 The `SlotRef` newtype

Single string format on the wire / CLI: `<provider_name>/<model>`. Examples:

```
anthropic/claude-opus-4-7
openai/gpt-4o
together/meta-llama/Llama-3.3-70B-Instruct-Turbo     ← model id contains '/'; provider name is the first segment up to the first '/'
```

`SlotRef::parse` splits on the **first** `/` only. Any further `/` characters belong to the model id. Provider names are restricted to `[a-z0-9-]+` (lowercase, no slashes) to make this unambiguous.

### 3.3 The `ProviderRegistry`

Constructed once per `xvn ab-compare` run. Reads:

1. The `providers` vec from `RuntimeConfig`.
2. CLI flag fallbacks (`--intern`, `--intern-model`, `--trader-base-url`, `--trader-model`, `--trader-api-key-env`) → synthesized as ad-hoc `ProviderEntry { name: "_cli_default_intern", … }` and `_cli_default_trader` rows if no real provider with those base_url/key matches.
3. The `[intern]` block → synthesized as `_default_intern` provider if not already named.

`intern_backend(&slot)` and `trader_backend(&slot)` look up the row by `slot.provider`, build an `OpenAiCompatBackend` / `AnthropicIntern` / `OpenAICompatIntern`, and memoize on `(provider_name, model)`. Calling them twice with the same SlotRef returns the same `Arc`.

Construction errors (provider name not found, env var missing) surface to the CLI with a single actionable message, e.g.:

```
provider "togetherz" referenced by arm "trader_arm:trader=togetherz/llama-3.3-70b" not registered.
known providers: anthropic, openai, together, ollama-local
add it to config/default.toml under [[providers]] or pass --provider-config <path>.
```

### 3.4 Per-arm wiring in `run_ab_compare`

```rust
ArmKind::Trader { intern, trader } => {
    let intern_slot = intern.unwrap_or_else(|| registry.default_intern.clone());
    let trader_slot = trader.unwrap_or_else(|| registry.default_trader.clone());

    let intern_backend = registry.intern_backend(&intern_slot)?;   // memoized
    let trader_backend = registry.trader_backend(&trader_slot)?;   // memoized

    Box::new(TraderArm::new(
        static_name,
        intern_backend,
        intern_slot.provider.clone(),
        intern_slot.model.clone(),
        Arc::clone(&cache),                  // SHARED across all arms — see §3.5
        trader_backend,
        trader_params.clone(),
        Arc::clone(&portfolio_provider),
    ))
}
```

### 3.5 BriefingCache divergence semantics (unchanged behavior, newly surfaced)

The cache is a workspace-level `Arc<BriefingCache>` shared across every TraderArm in a run. The key is `(cycle_id, intern_provider, intern_model)`.

| Scenario | What happens |
|---|---|
| Two arms with the same Intern slot, different Trader slots | Same cache key → Stage 1 fires once per cycle_id, briefings are reused → cheap A/B. |
| Two arms with different Intern slots | Different cache keys → Stage 1 fires twice per cycle_id → each Trader receives an independent briefing → expensive A/B. |
| Two arms with the same Intern *and* Trader slots | Cache hits at Stage 1; Stage 2 still fires per arm (Trader is not cached, by design — it sees portfolio state which mutates between calls). |

This is **already true** today. v1 surfaces it as a UI hint chip on the Intern Provider/Model selects: `Changes here re-run Stage 1 for every setup (cost ↑↑)`. The Trader select gets the inverse hint: `Changes here are cheap — Stage 1 is reused`.

Tested with a new `cache_diverges_on_intern_model_change` test in `xvision-eval/src/baselines/trader_arm.rs`.

---

## 4. Config schema

### 4.1 New `config/default.toml` shape (additive)

```toml
[runtime]
mode        = "backtest"
executor    = "alpaca"
random_seed = 42

[[providers]]
name        = "anthropic"
kind        = "anthropic"
base_url    = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[[providers]]
name        = "openai"
kind        = "openai-compat"
base_url    = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[[providers]]
name        = "together"
kind        = "openai-compat"
base_url    = "https://api.together.xyz/v1"
api_key_env = "TOGETHER_API_KEY"

[[providers]]
name        = "ollama-local"
kind        = "openai-compat"
base_url    = "http://localhost:11434/v1"
api_key_env = ""                    # empty = no auth header

# Existing [intern] block unchanged. On load, an auto-derived provider row
# is synthesized if no [[providers]] entry already matches its triple.
[intern]
provider          = "anthropic"
base_url          = "https://api.anthropic.com"
model             = "claude-haiku-4-5"
api_key_env       = "ANTHROPIC_API_KEY"
temperature       = 0.0
reasoning_effort  = "low"
max_tokens        = 1024

# … rest of file unchanged
```

### 4.2 `ProviderEntry` validation rules (`garde`)

- `name`: `length(min=1, max=32)`, regex `[a-z0-9-]+`.
- `kind`: enum from `{anthropic, openai-compat, local-candle}`.
- `base_url`: `length(min=1, max=512)`, must parse as `url::Url` with a non-empty host (except for `local-candle` which accepts a filesystem path).
- `api_key_env`: `length(max=64)`, may be empty (means no auth header).
- Cross-field: `name` must be unique within the `providers` vec. Validated post-parse with an actionable error.

### 4.3 Auto-derivation from `[intern]`

```rust
fn auto_derive_intern_provider_row(cfg: &mut RuntimeConfig) {
    let triple = (
        cfg.intern.provider,
        cfg.intern.base_url.clone(),
        cfg.intern.api_key_env.clone(),
    );
    if cfg.providers.iter().any(|p| p.matches_triple(&triple)) {
        return;  // user already declared it; no-op
    }
    let synthetic_name = format!("_default_intern");
    cfg.providers.push(ProviderEntry {
        name: synthetic_name,
        kind: cfg.intern.provider.into(),
        base_url: cfg.intern.base_url.clone(),
        api_key_env: cfg.intern.api_key_env.clone(),
    });
}
```

The synthetic row's name (`_default_intern`) is reserved — `garde` rejects user-declared rows with a name starting with `_`. This prevents name collisions and makes the synthetic row visible to anyone who lists providers.

---

## 5. ArmSpec extension

### 5.1 Grammar

```
arm_spec        := head (':' kv_pair)*
head            := identifier
kv_pair         := key '=' value
key             := identifier
value           := slot_ref | bare_value
slot_ref        := provider_name '/' model_id
provider_name   := [a-z0-9-]+
model_id        := [^,:]+                       # any string with no ',' or ':'
bare_value      := [^,:]+
```

For `trader_arm`:

| Key | Type | Default |
|---|---|---|
| `intern` | slot_ref | from CLI flags / `[intern]` block |
| `trader` | slot_ref | from CLI flags / fallback |
| `intern_model` | bare model id | implies `intern.provider = default_intern_provider`, model from value |
| `trader_model` | bare model id | implies `trader.provider = default_trader_provider`, model from value |

`intern=…` and `intern_model=…` are mutually exclusive; same for trader. Validated post-parse.

### 5.2 Examples

```
trader_arm                                                  # today's behavior
trader_arm:intern=anthropic/claude-opus-4-7                  # change just Intern
trader_arm:trader=openai/gpt-4o                              # change just Trader
trader_arm:intern=anthropic/claude-opus-4-7:trader=openai/gpt-4o
trader_arm:trader_model=gpt-4o-mini                          # use default Trader provider, override model

xvn ab-compare \
  --arms 'trader_arm:trader=anthropic/claude-opus-4-7,trader_arm:trader=openai/gpt-4o,trader_arm:trader=together/meta-llama/Llama-3.3-70B-Instruct-Turbo' \
  …
```

### 5.3 Arm naming

Each arm needs a unique name in the `BacktestResult`. When two arm specs have the same head (`trader_arm`), the parser auto-suffixes:

```
trader_arm:trader=anthropic/claude-opus-4-7    →  arm_name = "trader_arm[claude-opus-4-7]"
trader_arm:trader=openai/gpt-4o                →  arm_name = "trader_arm[gpt-4o]"
trader_arm:trader=together/meta-llama/Llama-3.3-70B-Instruct-Turbo  →  arm_name = "trader_arm[Llama-3.3-70B-Instruct-Turbo]"
```

Suffix rule: take the **last segment** of `model_id` (substring after the last `/` if any), trim to 32 chars. If two arms still collide after suffixing (same model from two providers), append the provider: `trader_arm[gpt-4o@openai]`.

Bare `trader_arm` keeps the name `trader_arm` (no suffix) so existing reports / scripts that key on that string don't break.

---

## 6. CLI surface

### 6.1 No new flags on `xvn ab-compare`

The existing `--intern`, `--intern-model`, `--trader-base-url`, `--trader-model`, `--trader-api-key-env` flags continue to do exactly what they do today. Their semantics shift from "applied to every TraderArm" to "default for any TraderArm without an inline `intern=` / `trader=` override." Existing scripts and CI runs are unaffected.

### 6.2 New CLI: `xvn provider`

```
xvn provider list                        # prints one row per registered provider, with key-set status
xvn provider show --name <name>          # full row + key-set status + last-resolved-at (from config mtime)
xvn provider check --name <name>         # makes a /models or /chat/completions ping; returns OK | <err>
xvn provider add --name … --kind … --base-url … --api-key-env …   # appends to config/default.toml
xvn provider remove --name …             # removes; refuses if any [intern] / [[providers]] reference it
```

`provider list` example:

```
NAME             KIND            BASE_URL                             API_KEY_ENV          KEY
anthropic        anthropic       https://api.anthropic.com            ANTHROPIC_API_KEY    ● set
openai           openai-compat   https://api.openai.com/v1            OPENAI_API_KEY       ● set
together         openai-compat   https://api.together.xyz/v1          TOGETHER_API_KEY     ○ missing
ollama-local     openai-compat   http://localhost:11434/v1            (none)               n/a
_default_intern  anthropic       https://api.anthropic.com            ANTHROPIC_API_KEY    ● set   (synthetic)
```

### 6.3 Tracing emitted on each arm dispatch

```
INFO ab_compare: arm=trader_arm[claude-opus-4-7] intern=anthropic/claude-haiku-4-5 trader=anthropic/claude-opus-4-7
INFO ab_compare: arm=trader_arm[gpt-4o]          intern=anthropic/claude-haiku-4-5 trader=openai/gpt-4o
INFO ab_compare: cache: 1 unique intern slot → briefings shared across 2 arm(s)
```

The third line tells the user at a glance whether their A/B is sharing Stage 1 (cheap) or not (expensive).

---

## 7. UI surface (design lock)

### 7.1 `/settings → 13.1 Providers`

Replaces the v0.1 "LLM keys" section. Layout:

| Region | Contents |
|---|---|
| Page header | `Providers` · `+ Add provider` primary button |
| Table (sortable) | columns: `Name`, `Kind`, `Base URL`, `API key env`, `Key`, `Used by`, `Actions` |
| Per row — Key chip | `● set` (green) when `std::env::var(name).is_ok()`, `○ missing` (amber) otherwise, `n/a` (grey) when env name is empty |
| Per row — Used by | count + tooltip listing slot references (e.g. `2 slots: workspace default Intern, draft btc-momentum.trader`) |
| Per row — Actions | `Edit`, `Delete` (disabled with tooltip when `Used by > 0`), `Test` (calls `/models` ping) |
| Empty state | `No providers yet. Add Anthropic, OpenAI, or any OpenAI-compatible endpoint.` + same three quick-link buttons as the v0.1 first-run modal |
| Add modal | fields: `Name` (regex-validated), `Kind` (select), `Base URL`, `API key env` (with `Detect` ghost button: tries common env names like `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`), `Test connection` ghost. Submit writes to `config/default.toml [[providers]]`. |

The first-run modal at `/setup` (`ui-elements.md` §3.3) keeps its current shape (single key paste) but the saved key materializes as a `[[providers]]` row, not a `[intern]` block.

### 7.2 Inspector LLM slot section (`ui-elements.md` §4.2.2)

Slot form gains one row above `Model`:

| Field | Label | Control |
|---|---|---|
| Provider | `Provider` | select sourced from `/settings → Providers`, with `+ Add new…` last item that opens the add-provider modal inline |
| Model | `Model` | combobox (free-text + autocomplete suggestions per provider — Anthropic suggests claude-*, OpenAI suggests gpt-*, etc.); the existing `Model` select label moves here |

Below the Model field, a **cost cue chip** (Move E surface):

- For `Intern` and `Regime` slots: `Changes here re-run Stage 1 for every setup ($$ per arm)`.
- For `Trader` slot: `Changes here are cheap — Stage 1 is reused across arms`.

The chip is informational, dismissable per-session. It quotes the BriefingCache rule.

### 7.3 `/strategies` row action `Fork with different model →`

Adds one item to the `⋯` action menu (`ui-elements.md` §5):

- Item: `Fork with different model →`
- Behavior: opens the Inspector (`/authoring/<new_draft_id>`) with the source draft duplicated, parentage recorded (`Forked from` column populated), AND the focus pinned on the Trader slot's `Model` select with the dropdown already expanded. The Provider select sits alongside and is also primed.
- Default arm name in the new draft: `<source_name>-<new_model_short>` (e.g. `btc-momentum-gpt-4o`).

Existing `Duplicate` and `Fork` items keep their current behavior; `Fork with different model →` is a *focused* fork — nothing else gets edited.

### 7.4 Lineage cue

The Control Tower lineage cue (`ui-elements.md` §2.3.3) gains a model-aware variant when ≥3 model-only forks exist from one root:

```
You've A/B-tested btc-momentum across 4 models this week — see leaderboard →
```

Links to `/eval/compare?ids=<lineage_root>` filtered by parent. (The compare view itself is already specced; this just adds an entry point.)

---

## 8. Sequencing

Within the hackathon timeline (deadline 2026-06-15), the work fits in one focused week:

| Day | Deliverable |
|---|---|
| D1 | `ProviderEntry` + `[[providers]]` config schema landed; `[intern]` auto-derivation; round-trip serde + garde tests passing. |
| D2 | `SlotRef` newtype + `parse_arm_spec` extension; arm-naming auto-suffix logic. Unit tests for the grammar including the multi-`/` edge case. |
| D3 | `ProviderRegistry` with backend memoization. `run_ab_compare` rewired. `xvn ab-compare` integration test exercises 3-model fork against mock backends. |
| D4 | `xvn provider {list,show,check,add,remove}` subcommand. `cache_diverges_on_intern_model_change` test landed. Tracing lines emitted. |
| D5 | UI design lock merged into `docs/design/ui-elements.md`: `/settings → Providers` revised section, Inspector slot Provider+Model dropdowns, `Fork with different model →` action, lineage-cue variant. |
| D6–D7 | Buffer for review feedback + a real-world smoke test running 3 actual models on a 1-day setup population. |

Total estimate: ~5 person-days of work + 2 days slack. Fits comfortably within the autooptimizer / marketplace plugin parallel tracks.

---

## 9. Failure modes and mitigations

| Failure | Mitigation |
|---|---|
| User passes a `provider/model` arm spec but the provider isn't registered. | CLI errors at parse time with the message in §3.3 listing known providers. `xvn provider add` is one command away. |
| Two arms inadvertently share a name after auto-suffix (same model id from two providers). | Auto-suffix appends `@<provider>`. If still colliding (impossible by construction since `(provider, model)` is unique per arm), the parser rejects with `arm names must be unique within a run`. |
| Env var for `api_key_env` is unset at backend construction. | `OpenAiCompatBackend::from_env` already returns `MissingApiKey(name)`; surfaced to the CLI with the offending arm name and provider name pre-pended. |
| User edits `[intern]` and adds an explicit `[[providers]]` row that matches its triple. | Auto-derivation skips when `matches_triple` returns true. The existing user row wins. |
| User-named provider starts with `_` (collides with synthetic-namespace reservation). | `garde` rejects with `provider names starting with '_' are reserved`. |
| User passes the same arm spec twice (`trader_arm:trader=openai/gpt-4o,trader_arm:trader=openai/gpt-4o`). | Allowed (e.g. for retry / variance studies); auto-suffix produces `trader_arm[gpt-4o]` and `trader_arm[gpt-4o]#2`. |
| `BriefingCache` is shared across arms with very different Intern models, blowing memory on long runs. | The cache is per-run and bounded by `cycle_id × distinct_intern_slots`. With ≤8 arms and ≤1k setups (current scale), worst case is ~8k cached briefings × ~2KB each ≈ 16MB. Fine for v1; revisit if it grows. |
| User runs `xvn provider remove --name X` while `[intern].provider = X`. | Refused with `cannot remove provider X: referenced by [intern] (workspace default Intern slot)`. User must change the slot first. |

---

## 10. Open questions (resolve in the implementation plan)

1. **Should `xvn provider check` actually fire a request, or is a TCP-connect smoke enough?** Real request gives a more honest signal but burns a token. Recommend: TCP-connect by default, `--probe` flag for a real `/models` ping.
2. **Where do `temperature`, `max_tokens`, `reasoning_effort` live now — on the provider, on the slot, or per-arm?** Currently on `[intern]`. Proposal: keep them on the slot (Inspector form fields) so per-arm overrides are natural; let the CLI flags continue to set workspace defaults. Spec assumes this; confirm during plan.
3. **Should `Regime` slot wiring ship in v1?** UI design accommodates it; backend wiring is a separate ADR. Recommend: lock the schema (so Regime appears in `ArmKind::Trader`'s slot-resolver loop) but ship as no-op behavior until the regime-classifier-as-LLM ADR.
4. **How does `Fork with different model →` interact with the strategy-creation engine's draft → bundle lifecycle?** Recommend: it produces a `Draft` row identical to `Duplicate`'s, then opens the Inspector with the slot pre-selected. The lineage edge is recorded in the same `Forked from` column. Confirm during plan.
5. **Do we expose per-arm `temperature` in the CLI grammar?** Out of scope for v1 — backtest mandates `temperature=0` (Tier 1 fix #1/#2). Forward-paper would need it but is a separate path. Defer.
6. **Cost telemetry rollup at the arm level — does any of the v1 dashboard need it?** The Inspector already shows per-call cost. A rollup across arms would live on `/eval/runs/<run_id>` KPI tiles. Proposed: surface as a future panel; out of scope for this spec.

---

## 11. References

- `crates/xvision-eval/src/ab_compare.rs` — current arm-spec grammar and parser; this spec extends it.
- `crates/xvision-eval/src/baselines/trader_arm.rs:80-84` — the `BriefingCache` key that already supports per-arm Intern divergence.
- `crates/xvision-trader/src/backend.rs:36-45` — `OpenAiCompatBackend` shape (one base_url + model + key).
- `crates/xvision-intern/src/backend.rs` — `AnthropicIntern` and `OpenAICompatIntern` constructors used by the registry.
- `crates/xvision-core/src/config.rs:76-93` — current `[intern]` block schema; v1 adds `[[providers]]` alongside.
- `config/default.toml` — the file v1 extends with `[[providers]]` rows.
- `docs/design/ui-elements.md` §3.3, §4.2.2, §5, §13.1 — UI design lock targets.
- `docs/dashboard.md` §A1–A3 — the agent identity / Intern settings / Trader settings cards that this spec makes per-arm in scope.
- `decisions/0011-cv-extraction.md` — the ADR that collapsed the four-arm steering split into a single TraderArm; this spec re-introduces a different axis of arm differentiation (model, not vectors).
- `strategies/README.md` — strategy backlog; this spec is what makes `bb_meanrev_zscore.md × {Claude, GPT-4o, Llama}` an executable A/B rather than a manual config swap.
