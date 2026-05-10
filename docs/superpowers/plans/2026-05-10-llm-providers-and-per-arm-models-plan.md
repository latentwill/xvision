# LLM Providers & Per-Arm Models Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every `trader_arm` in `xvn ab-compare` independently selectable for Intern model and Trader model, sourced from a `[[providers]]` registry in `config/default.toml`, so a strategy can be backtested against N LLMs in one run.

**Architecture:** Add a provider registry to `RuntimeConfig` (additive — `[intern]` block keeps working via auto-derivation). Introduce a `SlotRef = { provider, model }` newtype. Extend `ArmKind::Trader` from a unit variant to a struct with `Option<SlotRef>` for intern + trader slots. Build a `ProviderRegistry` that memoizes one `Arc<dyn Backend>` per `(provider, model)` so duplicate slots share an HTTP client. Wire it through `run_ab_compare`. Add `xvn provider` subcommand + UI design-lock edits.

**Tech Stack:** Rust 2021 / `clap` 4 / `serde` + `garde` 0.22 / `toml` 0.9 / `reqwest` 0.13 / existing `tokio` async runtime.

**Spec:** [`docs/superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md`](../specs/2026-05-10-llm-providers-and-per-arm-models-design.md)

---

## Phase 1 — Config schema (Day 1)

### Task 1: ProviderEntry + ProviderKind types in xvision-core

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (top of file region around line 38, add after `ConfigError`)

- [ ] **Step 1: Write the failing test**

Append to `crates/xvision-core/src/config.rs` `mod tests`:

```rust
#[test]
fn provider_kind_round_trips_via_serde() {
    use ProviderKind::*;
    for k in [Anthropic, OpenaiCompat, LocalCandle] {
        let s = toml::to_string(&ProviderEntry {
            name: "p".into(),
            kind: k,
            base_url: "https://example.com".into(),
            api_key_env: "X".into(),
        })
        .unwrap();
        let back: ProviderEntry = toml::from_str(&s).unwrap();
        assert_eq!(back.kind, k, "round trip failed for {:?}", k);
    }
}

#[test]
fn provider_kind_serializes_to_kebab_case() {
    let v = toml::Value::try_from(ProviderKind::OpenaiCompat).unwrap();
    assert_eq!(v.as_str(), Some("openai-compat"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-core provider_kind_round_trips_via_serde provider_kind_serializes_to_kebab_case`
Expected: FAIL with `cannot find type 'ProviderEntry'` / `cannot find type 'ProviderKind'`.

- [ ] **Step 3: Add ProviderKind + ProviderEntry**

Add to `crates/xvision-core/src/config.rs` after the `ConfigError` enum (~line 38) and before the `// --- runtime ---` divider:

```rust
// --- providers --------------------------------------------------------------

/// One LLM provider, referenced by name from slot configs and arm specs.
/// `api_key_env` may be the empty string for endpoints that don't require auth
/// (local llama.cpp / Ollama / vLLM in --no-auth mode).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct ProviderEntry {
    #[garde(custom(validate_provider_name))]
    pub name: String,
    #[garde(skip)]
    pub kind: ProviderKind,
    #[garde(length(min = 1, max = 512))]
    pub base_url: String,
    #[garde(length(max = 64))]
    pub api_key_env: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Anthropic,
    OpenaiCompat,
    LocalCandle,
}

impl From<InternProvider> for ProviderKind {
    fn from(p: InternProvider) -> Self {
        match p {
            InternProvider::Anthropic => Self::Anthropic,
            InternProvider::OpenaiCompat => Self::OpenaiCompat,
            InternProvider::LocalCandle => Self::LocalCandle,
        }
    }
}

impl ProviderEntry {
    /// True iff this entry's kind/base_url/api_key_env triple matches the
    /// supplied tuple. Used by auto-derivation to skip when the user has
    /// already declared an equivalent row.
    pub fn matches_triple(&self, kind: ProviderKind, base_url: &str, api_key_env: &str) -> bool {
        self.kind == kind && self.base_url == base_url && self.api_key_env == api_key_env
    }
}

fn validate_provider_name(name: &String, _ctx: &()) -> garde::Result {
    if name.is_empty() || name.len() > 32 {
        return Err(garde::Error::new("provider name must be 1..=32 chars"));
    }
    if name.starts_with('_') {
        // The leading-underscore namespace is reserved for synthetic rows
        // (e.g. _default_intern auto-derived from the [intern] block).
        return Err(garde::Error::new(
            "provider names starting with '_' are reserved",
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(garde::Error::new(
            "provider name must match [a-z0-9-]+",
        ));
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p xvision-core provider_kind_round_trips_via_serde provider_kind_serializes_to_kebab_case`
Expected: PASS, both tests.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-core/src/config.rs
git commit -m "feat(core): add ProviderEntry + ProviderKind config types"
```

---

### Task 2: Add `providers` vec to RuntimeConfig

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (RuntimeConfig struct, ~line 40)

- [ ] **Step 1: Write the failing test**

Append to `mod tests`:

```rust
#[test]
fn runtime_config_round_trips_with_providers() {
    let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[[providers]]
name = "ollama-local"
kind = "openai-compat"
base_url = "http://localhost:11434/v1"
api_key_env = ""

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("with-providers.toml");
    std::fs::write(&path, toml_src).unwrap();
    let cfg = load_runtime(&path).unwrap();
    assert_eq!(cfg.providers.len(), 2 + 1, "two declared + one auto-derived");
    assert!(cfg.providers.iter().any(|p| p.name == "anthropic"));
    assert!(cfg.providers.iter().any(|p| p.name == "ollama-local"));
}

#[test]
fn runtime_config_loads_without_providers_block() {
    // Existing default.toml has no [[providers]] block; must still load.
    let cfg =
        load_runtime(&project_root().join("config/default.toml")).expect("must load");
    // After auto-derivation we always have at least the synthetic _default_intern row.
    assert!(cfg.providers.iter().any(|p| p.name == "_default_intern"));
}
```

Note: Task 3 lands `auto_derive_intern_provider_row`. To keep this task green standalone, replace the third `assert_eq!` with `assert!(cfg.providers.len() >= 2)` if running this task in isolation; the `_default_intern` assertion in the second test is also Task-3-dependent. **If executing tasks strictly sequentially**, write only the round-trip test in Task 2 and add the second test inside Task 3.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-core runtime_config_round_trips_with_providers`
Expected: FAIL with `no field 'providers' on type 'RuntimeConfig'`.

- [ ] **Step 3: Add `providers` field**

Modify `RuntimeConfig`:

```rust
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[garde(skip)]
    pub runtime: Runtime,
    #[serde(default)]                    // ← NEW: empty if absent
    #[garde(dive)]
    pub providers: Vec<ProviderEntry>,   // ← NEW
    #[garde(dive)]
    pub intern: Intern,
    #[garde(dive)]
    pub trader: Trader,
    #[garde(dive)]
    pub backtest: Backtest,
    #[garde(skip)]
    pub paths: Paths,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xvision-core runtime_config_round_trips_with_providers`
Expected: PASS (the round-trip test only — second test waits for Task 3).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-core/src/config.rs
git commit -m "feat(core): add providers vec to RuntimeConfig"
```

---

### Task 3: Auto-derive synthetic provider from `[intern]` block + uniqueness validation

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (`load_runtime`, ~line 269)

- [ ] **Step 1: Write the failing tests**

Append to `mod tests`:

```rust
#[test]
fn auto_derives_default_intern_provider() {
    let cfg = load_runtime(&project_root().join("config/default.toml"))
        .expect("must load");
    let synth = cfg
        .providers
        .iter()
        .find(|p| p.name == "_default_intern")
        .expect("synthetic _default_intern row must be present");
    assert_eq!(synth.base_url, cfg.intern.base_url);
    assert_eq!(synth.api_key_env, cfg.intern.api_key_env);
}

#[test]
fn auto_derive_skips_when_user_already_declared_match() {
    let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("user-already-declared.toml");
    std::fs::write(&path, toml_src).unwrap();
    let cfg = load_runtime(&path).unwrap();
    assert_eq!(cfg.providers.len(), 1, "synthetic must be skipped");
    assert_eq!(cfg.providers[0].name, "anthropic");
}

#[test]
fn rejects_duplicate_provider_names() {
    let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "p"
kind = "anthropic"
base_url = "https://a.example"
api_key_env = "A"

[[providers]]
name = "p"
kind = "openai-compat"
base_url = "https://b.example"
api_key_env = "B"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "K"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dup-names.toml");
    std::fs::write(&path, toml_src).unwrap();
    match load_runtime(&path) {
        Err(ConfigError::CrossField { message, .. }) => {
            assert!(message.contains("duplicate provider name"), "actual: {message}");
        }
        other => panic!("expected CrossField, got {other:?}"),
    }
}

#[test]
fn rejects_provider_name_with_underscore_prefix() {
    let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "_mine"
kind = "anthropic"
base_url = "https://a.example"
api_key_env = "A"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "K"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reserved-name.toml");
    std::fs::write(&path, toml_src).unwrap();
    match load_runtime(&path) {
        Err(ConfigError::Validation { .. }) => {}
        other => panic!("expected Validation, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p xvision-core auto_derives_default_intern_provider auto_derive_skips_when_user_already_declared_match rejects_duplicate_provider_names rejects_provider_name_with_underscore_prefix`
Expected: FAIL — first two assert on a synthetic row not yet generated; third fails on `expected CrossField`; fourth depends on the `validate_provider_name` from Task 1 (already in place — passes).

- [ ] **Step 3: Implement auto-derivation + uniqueness check**

Modify `load_runtime` in `config.rs`:

```rust
pub fn load_runtime(path: &Path) -> Result<RuntimeConfig, ConfigError> {
    let mut cfg: RuntimeConfig = read_toml(path)?;
    cfg.backtest
        .validate_step_vs_horizon()
        .map_err(|msg| ConfigError::CrossField {
            path: path.to_path_buf(),
            message: msg,
        })?;
    auto_derive_intern_provider_row(&mut cfg);
    validate_unique_provider_names(&cfg).map_err(|msg| ConfigError::CrossField {
        path: path.to_path_buf(),
        message: msg,
    })?;
    Ok(cfg)
}

/// Synthesize a `_default_intern` provider row from the `[intern]` block if no
/// existing row already matches its (kind, base_url, api_key_env) triple. The
/// reserved underscore prefix prevents user-declared collisions.
fn auto_derive_intern_provider_row(cfg: &mut RuntimeConfig) {
    let kind: ProviderKind = cfg.intern.provider.into();
    let base_url = cfg.intern.base_url.clone();
    let api_key_env = cfg.intern.api_key_env.clone();
    if cfg
        .providers
        .iter()
        .any(|p| p.matches_triple(kind, &base_url, &api_key_env))
    {
        return;
    }
    cfg.providers.push(ProviderEntry {
        name: "_default_intern".to_string(),
        kind,
        base_url,
        api_key_env,
    });
}

fn validate_unique_provider_names(cfg: &RuntimeConfig) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for p in &cfg.providers {
        if !seen.insert(p.name.as_str()) {
            return Err(format!("duplicate provider name `{}`", p.name));
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run all xvision-core tests**

Run: `cargo test -p xvision-core`
Expected: PASS — all tests including the four new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-core/src/config.rs
git commit -m "feat(core): auto-derive synthetic intern provider + uniqueness check"
```

---

### Task 4: Update `config/default.toml` with explicit `[[providers]]` rows

**Files:**
- Modify: `config/default.toml`

- [ ] **Step 1: Write the failing test**

Add to `crates/xvision-core/src/config.rs` `mod tests`:

```rust
#[test]
fn repo_default_toml_declares_anthropic_provider() {
    let cfg = load_runtime(&project_root().join("config/default.toml")).unwrap();
    let anthropic = cfg
        .providers
        .iter()
        .find(|p| p.name == "anthropic")
        .expect("repo default.toml must declare an `anthropic` provider row");
    assert_eq!(anthropic.kind, ProviderKind::Anthropic);
    assert_eq!(anthropic.api_key_env, "ANTHROPIC_API_KEY");
    // [intern] points at the same triple → no synthetic row should appear
    assert!(
        !cfg.providers.iter().any(|p| p.name == "_default_intern"),
        "synthetic should be skipped when user-declared match exists"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-core repo_default_toml_declares_anthropic_provider`
Expected: FAIL with `repo default.toml must declare an 'anthropic' provider row`.

- [ ] **Step 3: Edit `config/default.toml`**

Insert after the `[runtime]` block (before `[intern]`):

```toml
# Registered LLM providers, referenced by name from slot configs and arm specs.
# api_key_env is the env var NAME — values are read at runtime, never stored.
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
name        = "ollama-local"
kind        = "openai-compat"
base_url    = "http://localhost:11434/v1"
api_key_env = ""
```

- [ ] **Step 4: Run all xvision-core tests**

Run: `cargo test -p xvision-core`
Expected: PASS — Task 4 test now finds the row, Task 3's auto-derive-skip test also still passes (the `anthropic` row matches the `[intern]` triple).

- [ ] **Step 5: Commit**

```bash
git add config/default.toml crates/xvision-core/src/config.rs
git commit -m "feat(config): declare anthropic/openai/ollama-local providers in default.toml"
```

---

## Phase 2 — SlotRef + Arm grammar (Day 2)

### Task 5: SlotRef newtype with parse + Display

**Files:**
- Create: `crates/xvision-core/src/slot.rs`
- Modify: `crates/xvision-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-core/src/slot.rs`:

```rust
//! `SlotRef` — `<provider>/<model>` reference used to resolve a backend at run
//! time. Provider names are restricted to `[a-z0-9-]+` (Task 1 garde rule)
//! which keeps the first `/` unambiguous: everything before is the provider,
//! everything after is the model id (model ids may themselves contain `/`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum SlotParseError {
    #[error("slot ref must contain '/': got `{0}`")]
    MissingSlash(String),
    #[error("slot ref provider segment must be non-empty: got `{0}`")]
    EmptyProvider(String),
    #[error("slot ref model segment must be non-empty: got `{0}`")]
    EmptyModel(String),
}

impl SlotRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl FromStr for SlotRef {
    type Err = SlotParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (provider, model) = s
            .split_once('/')
            .ok_or_else(|| SlotParseError::MissingSlash(s.to_string()))?;
        if provider.is_empty() {
            return Err(SlotParseError::EmptyProvider(s.to_string()));
        }
        if model.is_empty() {
            return Err(SlotParseError::EmptyModel(s.to_string()));
        }
        Ok(Self::new(provider, model))
    }
}

impl fmt::Display for SlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple() {
        let s: SlotRef = "anthropic/claude-opus-4-7".parse().unwrap();
        assert_eq!(s.provider, "anthropic");
        assert_eq!(s.model, "claude-opus-4-7");
    }

    #[test]
    fn model_id_keeps_inner_slashes() {
        let s: SlotRef = "together/meta-llama/Llama-3.3-70B-Instruct-Turbo"
            .parse()
            .unwrap();
        assert_eq!(s.provider, "together");
        assert_eq!(s.model, "meta-llama/Llama-3.3-70B-Instruct-Turbo");
    }

    #[test]
    fn display_round_trips() {
        let s = SlotRef::new("openai", "gpt-4o");
        assert_eq!(s.to_string(), "openai/gpt-4o");
        let back: SlotRef = s.to_string().parse().unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn rejects_missing_slash() {
        assert_eq!(
            "noslash".parse::<SlotRef>(),
            Err(SlotParseError::MissingSlash("noslash".into()))
        );
    }

    #[test]
    fn rejects_empty_provider() {
        assert_eq!(
            "/model".parse::<SlotRef>(),
            Err(SlotParseError::EmptyProvider("/model".into()))
        );
    }

    #[test]
    fn rejects_empty_model() {
        assert_eq!(
            "provider/".parse::<SlotRef>(),
            Err(SlotParseError::EmptyModel("provider/".into()))
        );
    }
}
```

- [ ] **Step 2: Wire the module + run tests, verify they fail**

Add to `crates/xvision-core/src/lib.rs`:

```rust
pub mod slot;
```

Run: `cargo test -p xvision-core slot::`
Expected: FAIL initially if module isn't wired — once `pub mod slot;` is added, all tests should compile and PASS first try (the implementation is in Step 1 alongside the tests).

If you prefer strict TDD red-then-green: write only the tests in `slot.rs` first, run them (compile fails), then add the impl below the test module. The end state is identical.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p xvision-core slot::`
Expected: PASS — six tests.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-core/src/slot.rs crates/xvision-core/src/lib.rs
git commit -m "feat(core): add SlotRef newtype with <provider>/<model> parse + Display"
```

---

### Task 6: Extend `ArmKind::Trader` to a struct variant with optional slots

**Files:**
- Modify: `crates/xvision-eval/src/ab_compare.rs` (~line 38)

- [ ] **Step 1: Write the failing test**

Append to `crates/xvision-eval/src/ab_compare.rs` `mod tests`:

```rust
#[test]
fn trader_arm_kind_carries_optional_slots() {
    use xvision_core::slot::SlotRef;
    let spec = ArmSpec {
        name: "trader_arm".into(),
        kind: ArmKind::Trader {
            intern: Some(SlotRef::new("anthropic", "claude-opus-4-7")),
            trader: None,
        },
    };
    match spec.kind {
        ArmKind::Trader { intern, trader } => {
            assert_eq!(intern.unwrap().model, "claude-opus-4-7");
            assert!(trader.is_none());
        }
        _ => panic!("wrong variant"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-eval trader_arm_kind_carries_optional_slots`
Expected: FAIL with `expected unit variant 'ArmKind::Trader'` or similar.

- [ ] **Step 3: Add `xvision-core` import**

`crates/xvision-eval/src/ab_compare.rs` already depends on `xvision_core` for `MarketSnapshot`. Add the slot import near the top:

```rust
use xvision_core::slot::SlotRef;
```

- [ ] **Step 4: Convert `ArmKind::Trader` to struct variant**

In `crates/xvision-eval/src/ab_compare.rs`, change `ArmKind`:

```rust
#[derive(Debug, Clone)]
pub enum ArmKind {
    Trader {
        intern: Option<SlotRef>,
        trader: Option<SlotRef>,
    },
    BuyAndHold,
    AlwaysLong,
    AlwaysShort,
    RandomDirection { seed: u64 },
    RsiMeanReversion,
    MaCrossover { fast: usize, slow: usize },
    MacdMomentum,
}
```

Update the existing `parse_arm_spec` `"trader_arm"` case to:

```rust
"trader_arm" => Ok(ArmSpec {
    name: "trader_arm".into(),
    kind: ArmKind::Trader { intern: None, trader: None },
}),
```

Update the existing `default_arms()` first entry to `kind: ArmKind::Trader { intern: None, trader: None }`.

Update the existing `parse_trader_arm` test:

```rust
#[test]
fn parse_trader_arm() {
    let a = parse_arm_spec("trader_arm").unwrap();
    assert_eq!(a.name, "trader_arm");
    matches!(a.kind, ArmKind::Trader { intern: None, trader: None });
}
```

Update the `default_arms_includes_trader_and_buy_and_hold` test — no shape change needed; it only inspects names.

Update the `run_ab_compare` `match spec.kind` block in `crates/xvision-eval/src/ab_compare.rs` (~line 156):

```rust
ArmKind::Trader { intern: _, trader: _ } => Box::new(TraderArm::new(
    static_name,
    Arc::clone(&intern),
    intern_provider.clone(),
    intern_model.clone(),
    Arc::clone(&cache),
    Arc::clone(&trader),
    trader_params.clone(),
    Arc::clone(&portfolio_provider),
)),
```

The actual per-arm slot resolution lands in Task 11; for now the new fields are read-but-ignored so the type plumbing compiles cleanly.

- [ ] **Step 5: Run all xvision-eval tests**

Run: `cargo test -p xvision-eval`
Expected: PASS — all existing tests + the new `trader_arm_kind_carries_optional_slots`.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-eval/src/ab_compare.rs
git commit -m "refactor(eval): make ArmKind::Trader carry optional Intern/Trader slots"
```

---

### Task 7: Extend `parse_arm_spec` to accept `intern=` / `trader=` / `intern_model=` / `trader_model=`

**Files:**
- Modify: `crates/xvision-eval/src/ab_compare.rs` (`parse_arm_spec`, ~line 56)

- [ ] **Step 1: Write the failing tests**

Append to `mod tests`:

```rust
#[test]
fn parses_trader_arm_with_intern_slot() {
    let a = parse_arm_spec("trader_arm:intern=anthropic/claude-opus-4-7").unwrap();
    match a.kind {
        ArmKind::Trader { intern, trader } => {
            let s = intern.expect("intern slot must be present");
            assert_eq!(s.provider, "anthropic");
            assert_eq!(s.model, "claude-opus-4-7");
            assert!(trader.is_none());
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn parses_trader_arm_with_trader_slot_only() {
    let a = parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap();
    match a.kind {
        ArmKind::Trader { intern, trader } => {
            assert!(intern.is_none());
            let s = trader.expect("trader slot must be present");
            assert_eq!(s.provider, "openai");
            assert_eq!(s.model, "gpt-4o");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn parses_trader_arm_with_both_slots() {
    let a = parse_arm_spec(
        "trader_arm:intern=anthropic/claude-haiku-4-5:trader=openai/gpt-4o",
    )
    .unwrap();
    match a.kind {
        ArmKind::Trader { intern, trader } => {
            assert_eq!(intern.unwrap().model, "claude-haiku-4-5");
            assert_eq!(trader.unwrap().model, "gpt-4o");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn parses_trader_model_shorthand() {
    let a = parse_arm_spec("trader_arm:trader_model=gpt-4o-mini").unwrap();
    match a.kind {
        ArmKind::Trader { intern, trader } => {
            assert!(intern.is_none());
            // shorthand carries only the model; provider stays None and is
            // resolved from the CLI-flag default at registry time.
            let s = trader.expect("trader slot must be present");
            assert_eq!(s.provider, "");
            assert_eq!(s.model, "gpt-4o-mini");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn rejects_intern_and_intern_model_together() {
    let err =
        parse_arm_spec("trader_arm:intern=anthropic/x:intern_model=y").unwrap_err();
    assert!(format!("{err}").contains("mutually exclusive"));
}

#[test]
fn rejects_trader_arm_with_unknown_kv() {
    let err = parse_arm_spec("trader_arm:bogus=x").unwrap_err();
    assert!(format!("{err}").contains("unknown key"));
}
```

Note: the empty-provider trick (`SlotRef { provider: "", model: "..." }`) is the marker for "shorthand — fill in provider from CLI flag default". This is only ever produced by `intern_model=` / `trader_model=` shorthand and only ever consumed by the registry resolver in Task 11; no other code path sees it.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p xvision-eval parses_trader_arm_with parses_trader_model_shorthand rejects_intern_and rejects_trader_arm_with_unknown`
Expected: all six FAIL.

- [ ] **Step 3: Rewrite the `"trader_arm"` arm of `parse_arm_spec`**

Replace the existing `"trader_arm" => Ok(ArmSpec { … })` block with:

```rust
"trader_arm" => {
    let kv = parse_kv(rest);
    // Allowed keys; reject anything else so typos surface fast.
    const ALLOWED: &[&str] = &["intern", "trader", "intern_model", "trader_model"];
    for k in kv.keys() {
        if !ALLOWED.contains(&k.as_str()) {
            return Err(anyhow!("unknown key `{k}` in trader_arm spec"));
        }
    }
    if kv.contains_key("intern") && kv.contains_key("intern_model") {
        return Err(anyhow!(
            "`intern=` and `intern_model=` are mutually exclusive on trader_arm"
        ));
    }
    if kv.contains_key("trader") && kv.contains_key("trader_model") {
        return Err(anyhow!(
            "`trader=` and `trader_model=` are mutually exclusive on trader_arm"
        ));
    }
    let intern = match (kv.get("intern"), kv.get("intern_model")) {
        (Some(slot), _) => Some(
            slot.parse::<SlotRef>()
                .map_err(|e| anyhow!("intern slot ref: {e}"))?,
        ),
        (_, Some(model)) => Some(SlotRef::new("", model.clone())),
        _ => None,
    };
    let trader = match (kv.get("trader"), kv.get("trader_model")) {
        (Some(slot), _) => Some(
            slot.parse::<SlotRef>()
                .map_err(|e| anyhow!("trader slot ref: {e}"))?,
        ),
        (_, Some(model)) => Some(SlotRef::new("", model.clone())),
        _ => None,
    };
    Ok(ArmSpec {
        name: "trader_arm".into(),
        kind: ArmKind::Trader { intern, trader },
    })
}
```

Important: `parse_kv` uses `:` as separator (already true today). A slot like `anthropic/claude-opus-4-7` does not contain `:` so this is safe. If a model id ever contains `:`, the user can use the shorthand form (`trader_model=…`). Note this caveat in the open questions section of the spec — current model ids in the wild don't use `:`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p xvision-eval`
Expected: PASS — all existing tests + the six new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-eval/src/ab_compare.rs
git commit -m "feat(eval): parse intern=/trader= slot overrides on trader_arm spec"
```

---

### Task 8: Auto-suffix arm-naming logic for distinct-model arms

**Files:**
- Modify: `crates/xvision-eval/src/ab_compare.rs` (after `parse_arm_spec`)

- [ ] **Step 1: Write the failing tests**

Append to `mod tests`:

```rust
#[test]
fn auto_suffix_uses_last_path_segment_of_model() {
    let mut specs = vec![
        parse_arm_spec("trader_arm:trader=anthropic/claude-opus-4-7").unwrap(),
        parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
    ];
    auto_suffix_arm_names(&mut specs);
    assert_eq!(specs[0].name, "trader_arm[claude-opus-4-7]");
    assert_eq!(specs[1].name, "trader_arm[gpt-4o]");
}

#[test]
fn auto_suffix_strips_provider_path_from_model_id() {
    let mut specs = vec![
        parse_arm_spec(
            "trader_arm:trader=together/meta-llama/Llama-3.3-70B-Instruct-Turbo",
        )
        .unwrap(),
    ];
    auto_suffix_arm_names(&mut specs);
    assert_eq!(specs[0].name, "trader_arm[Llama-3.3-70B-Instruct-Turbo]");
}

#[test]
fn auto_suffix_appends_provider_when_models_collide() {
    let mut specs = vec![
        parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
        parse_arm_spec("trader_arm:trader=together/gpt-4o").unwrap(),
    ];
    auto_suffix_arm_names(&mut specs);
    assert_eq!(specs[0].name, "trader_arm[gpt-4o@openai]");
    assert_eq!(specs[1].name, "trader_arm[gpt-4o@together]");
}

#[test]
fn auto_suffix_leaves_bare_trader_arm_alone() {
    let mut specs = vec![
        parse_arm_spec("trader_arm").unwrap(),
        parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
    ];
    auto_suffix_arm_names(&mut specs);
    assert_eq!(specs[0].name, "trader_arm");
    assert_eq!(specs[1].name, "trader_arm[gpt-4o]");
}

#[test]
fn auto_suffix_ignores_non_trader_arms() {
    let mut specs = vec![
        parse_arm_spec("buy_and_hold").unwrap(),
        parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
    ];
    auto_suffix_arm_names(&mut specs);
    assert_eq!(specs[0].name, "buy_and_hold");
    assert_eq!(specs[1].name, "trader_arm[gpt-4o]");
}

#[test]
fn auto_suffix_handles_intern_only_override() {
    let mut specs = vec![
        parse_arm_spec("trader_arm:intern=anthropic/claude-opus-4-7").unwrap(),
        parse_arm_spec("trader_arm:intern=anthropic/claude-haiku-4-5").unwrap(),
    ];
    auto_suffix_arm_names(&mut specs);
    // When only intern differs, suffix is the intern model id.
    assert_eq!(specs[0].name, "trader_arm[i:claude-opus-4-7]");
    assert_eq!(specs[1].name, "trader_arm[i:claude-haiku-4-5]");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p xvision-eval auto_suffix_`
Expected: all six FAIL with `cannot find function 'auto_suffix_arm_names'`.

- [ ] **Step 3: Implement the suffix function**

Add to `crates/xvision-eval/src/ab_compare.rs` after `parse_kv`:

```rust
/// Mutates `specs` in place so any two `trader_arm` entries with distinct
/// slot configs end up with distinct names. Bare `trader_arm` (no slot
/// overrides) keeps its name unchanged so existing scripts/reports keep
/// working.
pub fn auto_suffix_arm_names(specs: &mut [ArmSpec]) {
    // Pass 1: derive the candidate suffix per spec.
    let mut suffixes: Vec<Option<String>> = specs
        .iter()
        .map(|spec| match &spec.kind {
            ArmKind::Trader { intern, trader } => match (trader, intern) {
                (None, None) => None, // bare trader_arm — leave alone
                (Some(t), _) => Some(short_model_segment(&t.model)),
                (None, Some(i)) => Some(format!("i:{}", short_model_segment(&i.model))),
            },
            _ => None,
        })
        .collect();

    // Pass 2: detect collisions on the candidate suffix and promote to
    // `<model>@<provider>` when needed. Only applies among `Trader` specs.
    let mut by_suffix: std::collections::HashMap<&String, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, s) in suffixes.iter().enumerate() {
        if let Some(s) = s {
            by_suffix.entry(s).or_default().push(i);
        }
    }
    let mut promotions: Vec<(usize, String)> = vec![];
    for (_, idxs) in by_suffix.iter().filter(|(_, v)| v.len() > 1) {
        for &i in idxs {
            if let ArmKind::Trader { trader, intern } = &specs[i].kind {
                let (model, provider) = match (trader, intern) {
                    (Some(t), _) => (&t.model, &t.provider),
                    (None, Some(j)) => (&j.model, &j.provider),
                    _ => continue,
                };
                let suffix = format!("{}@{}", short_model_segment(model), provider);
                promotions.push((i, suffix));
            }
        }
    }
    for (i, suf) in promotions {
        suffixes[i] = Some(suf);
    }

    // Pass 3: apply.
    for (spec, suffix) in specs.iter_mut().zip(suffixes.into_iter()) {
        if let Some(suf) = suffix {
            spec.name = format!("{}[{}]", spec.name, suf);
        }
    }
}

/// `meta-llama/Llama-3.3-70B-Instruct-Turbo` → `Llama-3.3-70B-Instruct-Turbo`.
/// Trims to 32 chars to keep BacktestResult arm names readable.
fn short_model_segment(model: &str) -> String {
    let last = model.rsplit('/').next().unwrap_or(model);
    let trimmed: String = last.chars().take(32).collect();
    trimmed
}
```

Then wire it into the CLI flow — modify `crates/xvision-cli/src/commands/ab_compare.rs` after the `parse_arm_spec` loop (~line 44):

```rust
let mut arm_specs: Vec<_> = if arms.trim().is_empty() {
    default_arms()
} else {
    arms.split(',')
        .map(|s| parse_arm_spec(s.trim()))
        .collect::<anyhow::Result<Vec<_>>>()?
};
xvision_eval::ab_compare::auto_suffix_arm_names(&mut arm_specs);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p xvision-eval`
Expected: PASS — all existing tests + the six new ones. Build the CLI to make sure the wiring compiles: `cargo build -p xvision-cli`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-eval/src/ab_compare.rs crates/xvision-cli/src/commands/ab_compare.rs
git commit -m "feat(eval): auto-suffix trader_arm names when slot overrides differ"
```

---

## Phase 3 — ProviderRegistry + run_ab_compare wiring (Day 3)

### Task 9: ProviderRegistry — struct + intern_backend memoizer

**Files:**
- Create: `crates/xvision-eval/src/provider_registry.rs`
- Modify: `crates/xvision-eval/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-eval/src/provider_registry.rs`:

```rust
//! `ProviderRegistry` — resolves `SlotRef` to backend `Arc`s, memoizing one
//! instance per `(provider, model)` so two arms sharing a slot share an HTTP
//! client. See spec §3.3.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::slot::SlotRef;
use xvision_intern::backend::{AnthropicIntern, InternBackend, OpenAICompatIntern};
use xvision_trader::{OpenAiCompatBackend, TraderBackend};

pub struct ProviderRegistry {
    rows: Vec<ProviderEntry>,
    pub default_intern: SlotRef,
    pub default_trader: SlotRef,
    intern_cache: Mutex<HashMap<(String, String), Arc<dyn InternBackend>>>,
    trader_cache: Mutex<HashMap<(String, String), Arc<dyn TraderBackend>>>,
}

impl ProviderRegistry {
    pub fn new(
        rows: Vec<ProviderEntry>,
        default_intern: SlotRef,
        default_trader: SlotRef,
    ) -> Self {
        Self {
            rows,
            default_intern,
            default_trader,
            intern_cache: Mutex::new(HashMap::new()),
            trader_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn rows(&self) -> &[ProviderEntry] {
        &self.rows
    }

    /// Resolve an intern slot to a backend Arc, filling in the provider from
    /// `default_intern.provider` when the slot's provider segment is empty
    /// (the shorthand-form marker emitted by `parse_arm_spec`).
    pub fn intern_backend(&self, slot: &SlotRef) -> Result<Arc<dyn InternBackend>> {
        let resolved = self.fill_default_provider(slot, &self.default_intern);
        let key = (resolved.provider.clone(), resolved.model.clone());
        {
            let cache = self.intern_cache.lock().unwrap();
            if let Some(b) = cache.get(&key) {
                return Ok(Arc::clone(b));
            }
        }
        let row = self.find_provider(&resolved.provider, "intern")?;
        let backend: Arc<dyn InternBackend> = match row.kind {
            ProviderKind::Anthropic => Arc::new(AnthropicIntern::from_env(
                row.base_url.clone(),
                &resolved.model,
                &row.api_key_env,
            )?),
            ProviderKind::OpenaiCompat => Arc::new(OpenAICompatIntern::from_env(
                row.base_url.clone(),
                &resolved.model,
                &row.api_key_env,
            )?),
            ProviderKind::LocalCandle => {
                return Err(anyhow!(
                    "provider `{}` kind=local-candle is not yet supported as an Intern slot",
                    resolved.provider
                ));
            }
        };
        self.intern_cache
            .lock()
            .unwrap()
            .insert(key, Arc::clone(&backend));
        Ok(backend)
    }

    fn fill_default_provider(&self, slot: &SlotRef, default: &SlotRef) -> SlotRef {
        if slot.provider.is_empty() {
            SlotRef::new(default.provider.clone(), slot.model.clone())
        } else {
            slot.clone()
        }
    }

    fn find_provider(&self, name: &str, role: &str) -> Result<&ProviderEntry> {
        self.rows.iter().find(|p| p.name == name).ok_or_else(|| {
            let known: Vec<&str> = self.rows.iter().map(|p| p.name.as_str()).collect();
            anyhow!(
                "provider `{name}` referenced by {role} slot not registered.\n\
                 known providers: {}\n\
                 add it to config/default.toml under [[providers]].",
                known.join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(rows: Vec<ProviderEntry>) -> ProviderRegistry {
        ProviderRegistry::new(
            rows,
            SlotRef::new("anthropic", "claude-haiku-4-5"),
            SlotRef::new("openai", "gpt-4o"),
        )
    }

    #[test]
    fn missing_provider_yields_actionable_error() {
        let reg = registry_with(vec![]);
        let err = reg
            .intern_backend(&SlotRef::new("nope", "x"))
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("nope"), "actual: {msg}");
        assert!(msg.contains("known providers"), "actual: {msg}");
    }

    #[test]
    fn unknown_kind_local_candle_errors_for_intern() {
        let row = ProviderEntry {
            name: "local".into(),
            kind: ProviderKind::LocalCandle,
            base_url: "models/x.gguf".into(),
            api_key_env: "".into(),
        };
        let reg = registry_with(vec![row]);
        let err = reg
            .intern_backend(&SlotRef::new("local", "x"))
            .unwrap_err();
        assert!(format!("{err:#}").contains("local-candle"));
    }
}
```

- [ ] **Step 2: Wire the module + run tests**

Add to `crates/xvision-eval/src/lib.rs`:

```rust
pub mod provider_registry;
```

Run: `cargo test -p xvision-eval provider_registry::`
Expected: tests compile and PASS — both run real instantiation paths that don't hit the network. (`AnthropicIntern::from_env` is only called for the missing-key happy path, which we don't exercise in this task; the failure paths above are covered.)

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-eval/src/provider_registry.rs crates/xvision-eval/src/lib.rs
git commit -m "feat(eval): ProviderRegistry skeleton with intern_backend resolver"
```

---

### Task 10: ProviderRegistry — trader_backend memoizer + memoization test

**Files:**
- Modify: `crates/xvision-eval/src/provider_registry.rs`

- [ ] **Step 1: Write the failing tests**

Append to `mod tests`:

```rust
#[test]
fn intern_backend_memoizes_on_provider_model() {
    use std::env;
    env::set_var("DUMMY_KEY", "k");
    let row = ProviderEntry {
        name: "openai".into(),
        kind: ProviderKind::OpenaiCompat,
        base_url: "https://api.openai.com/v1".into(),
        api_key_env: "DUMMY_KEY".into(),
    };
    let reg = ProviderRegistry::new(
        vec![row],
        SlotRef::new("openai", "gpt-4o"),
        SlotRef::new("openai", "gpt-4o"),
    );
    let a = reg
        .intern_backend(&SlotRef::new("openai", "gpt-4o"))
        .unwrap();
    let b = reg
        .intern_backend(&SlotRef::new("openai", "gpt-4o"))
        .unwrap();
    assert!(Arc::ptr_eq(&a, &b), "same slot must yield the same Arc");
    let c = reg
        .intern_backend(&SlotRef::new("openai", "gpt-4o-mini"))
        .unwrap();
    assert!(!Arc::ptr_eq(&a, &c), "different model must yield a different Arc");
}

#[test]
fn trader_backend_memoizes_on_provider_model() {
    use std::env;
    env::set_var("DUMMY_KEY", "k");
    let row = ProviderEntry {
        name: "openai".into(),
        kind: ProviderKind::OpenaiCompat,
        base_url: "https://api.openai.com/v1".into(),
        api_key_env: "DUMMY_KEY".into(),
    };
    let reg = ProviderRegistry::new(
        vec![row],
        SlotRef::new("openai", "gpt-4o"),
        SlotRef::new("openai", "gpt-4o"),
    );
    let a = reg
        .trader_backend(&SlotRef::new("openai", "gpt-4o"))
        .unwrap();
    let b = reg
        .trader_backend(&SlotRef::new("openai", "gpt-4o"))
        .unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn empty_provider_in_slot_falls_back_to_default() {
    use std::env;
    env::set_var("DUMMY_KEY", "k");
    let row = ProviderEntry {
        name: "openai".into(),
        kind: ProviderKind::OpenaiCompat,
        base_url: "https://api.openai.com/v1".into(),
        api_key_env: "DUMMY_KEY".into(),
    };
    let reg = ProviderRegistry::new(
        vec![row],
        SlotRef::new("openai", "default-model"),
        SlotRef::new("openai", "default-trader"),
    );
    // Empty provider segment ("", "gpt-4o-mini") — comes from `trader_model=`
    // shorthand. Resolver should fill in the default Trader provider.
    let backend = reg
        .trader_backend(&SlotRef::new("", "gpt-4o-mini"))
        .unwrap();
    // Ask again with explicit provider — should hit the same cache entry.
    let backend2 = reg
        .trader_backend(&SlotRef::new("openai", "gpt-4o-mini"))
        .unwrap();
    assert!(Arc::ptr_eq(&backend, &backend2));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p xvision-eval provider_registry::tests::intern_backend_memoizes provider_registry::tests::trader_backend_memoizes provider_registry::tests::empty_provider_in_slot`
Expected: FAIL — `trader_backend` doesn't exist yet, intern_backend works for the first test but trader/empty-provider tests need new code.

- [ ] **Step 3: Add `trader_backend` method**

Add to `impl ProviderRegistry` in `provider_registry.rs`:

```rust
pub fn trader_backend(&self, slot: &SlotRef) -> Result<Arc<dyn TraderBackend>> {
    let resolved = self.fill_default_provider(slot, &self.default_trader);
    let key = (resolved.provider.clone(), resolved.model.clone());
    {
        let cache = self.trader_cache.lock().unwrap();
        if let Some(b) = cache.get(&key) {
            return Ok(Arc::clone(b));
        }
    }
    let row = self.find_provider(&resolved.provider, "trader")?;
    let backend: Arc<dyn TraderBackend> = match row.kind {
        ProviderKind::Anthropic => {
            return Err(anyhow!(
                "provider `{}` kind=anthropic is not yet supported as a Trader slot \
                 (Anthropic Messages API has a different shape than the Chat Completions \
                 contract OpenAiCompatBackend implements). Use openai-compat with an \
                 Anthropic-compatible gateway, or wait for AnthropicTraderBackend.",
                resolved.provider
            ));
        }
        ProviderKind::OpenaiCompat => Arc::new(OpenAiCompatBackend::from_env(
            row.base_url.clone(),
            &resolved.model,
            &row.api_key_env,
        )?),
        ProviderKind::LocalCandle => {
            return Err(anyhow!(
                "provider `{}` kind=local-candle is not yet supported as a Trader slot",
                resolved.provider
            ));
        }
    };
    self.trader_cache
        .lock()
        .unwrap()
        .insert(key, Arc::clone(&backend));
    Ok(backend)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p xvision-eval provider_registry::`
Expected: PASS — all tests including the three new memoization tests.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-eval/src/provider_registry.rs
git commit -m "feat(eval): trader_backend resolver with per-slot memoization"
```

---

### Task 11: Wire ProviderRegistry into `run_ab_compare`

**Files:**
- Modify: `crates/xvision-eval/src/ab_compare.rs` (`run_ab_compare`, ~line 134)

- [ ] **Step 1: Write the failing test**

Add to `crates/xvision-eval/src/ab_compare.rs` `mod tests`:

```rust
// MockBackend imports — same shape as the existing MockBackend in
// xvision-trader/src/backend.rs tests, but accessible from the eval crate
// via dependency. We rebuild a minimal one here to keep the test isolated.
#[cfg(test)]
mod registry_wiring {
    use super::*;
    use std::sync::{Arc, Mutex};
    use xvision_core::slot::SlotRef;

    use xvision_intern::backend::InternBackend;
    use xvision_trader::backend::TraderBackend;
    use xvision_trader::error::TraderError;
    use xvision_intern::backend::InternError;

    struct MockTrader { calls: Mutex<Vec<String>> }
    #[async_trait::async_trait]
    impl TraderBackend for MockTrader {
        async fn complete(&self, p: &str) -> Result<String, TraderError> {
            self.calls.lock().unwrap().push(p.into());
            Ok(r#"{"action":"flat","reason":"mock","cycle_id":"00000000-0000-0000-0000-000000000000"}"#.into())
        }
    }

    // Rather than building a full backtest, this test only checks that
    // resolve_intern_for_arm() / resolve_trader_for_arm() return the
    // ProviderRegistry-issued backend Arcs for per-arm overrides, falling
    // back to the registry default when the slot is None.
    #[test]
    fn resolve_uses_registry_with_per_arm_override_when_present() {
        std::env::set_var("DUMMY_KEY", "k");
        let rows = vec![
            xvision_core::config::ProviderEntry {
                name: "openai".into(),
                kind: xvision_core::config::ProviderKind::OpenaiCompat,
                base_url: "https://api.openai.com/v1".into(),
                api_key_env: "DUMMY_KEY".into(),
            },
        ];
        let registry = std::sync::Arc::new(crate::provider_registry::ProviderRegistry::new(
            rows,
            SlotRef::new("openai", "gpt-4o"),
            SlotRef::new("openai", "gpt-4o"),
        ));
        let default_t = registry.trader_backend(&registry.default_trader).unwrap();
        let override_t = registry.trader_backend(&SlotRef::new("openai", "gpt-4o-mini")).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&default_t, &override_t));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-eval registry_wiring::`
Expected: FAIL (compile fails — registry-wiring is purely a sanity check that the resolver is reachable from `ab_compare.rs`; it should compile and pass once the import is in place. If your strict-TDD discipline insists on a red, omit `#[cfg(test)]` and the inner `mod registry_wiring {}` for one cycle and watch the compile error.)

- [ ] **Step 3: Rewrite `run_ab_compare` to take a `ProviderRegistry`**

Replace the `run_ab_compare` signature + body. Keep an external compatibility shim so existing callers don't break in this task — Task 12 swaps the CLI to the new signature.

```rust
use crate::provider_registry::ProviderRegistry;

/// New signature: takes a `ProviderRegistry`. Slot overrides on each
/// `ArmKind::Trader` resolve through the registry; `None` falls back to the
/// registry's default Intern / Trader slots.
#[allow(clippy::too_many_arguments)]
pub async fn run_ab_compare_v2(
    snapshots: Vec<MarketSnapshot>,
    bars: Vec<MarketBar>,
    arms: Vec<ArmSpec>,
    config: BacktestRunConfig,
    registry: Arc<ProviderRegistry>,
    trader_params: TraderParams,
    portfolio_provider: PortfolioProvider,
    risk: &RiskLayer,
) -> anyhow::Result<BacktestResult> {
    let cache = Arc::new(BriefingCache::new());

    let arm_configs: Vec<ArmConfig> = arms
        .into_iter()
        .map(|spec| -> anyhow::Result<ArmConfig> {
            let static_name: &'static str =
                Box::leak(spec.name.clone().into_boxed_str());
            let strategy: Box<dyn Strategy> = match spec.kind {
                ArmKind::Trader { intern, trader } => {
                    let intern_slot = intern.unwrap_or_else(|| registry.default_intern.clone());
                    let trader_slot = trader.unwrap_or_else(|| registry.default_trader.clone());
                    let intern_backend = registry.intern_backend(&intern_slot)?;
                    let trader_backend = registry.trader_backend(&trader_slot)?;
                    let resolved_intern = if intern_slot.provider.is_empty() {
                        SlotRef::new(
                            registry.default_intern.provider.clone(),
                            intern_slot.model.clone(),
                        )
                    } else {
                        intern_slot
                    };
                    tracing::info!(
                        target: "ab_compare",
                        arm = %spec.name,
                        intern = %resolved_intern,
                        trader = %trader_slot,
                        "arm dispatch"
                    );
                    Box::new(TraderArm::new(
                        static_name,
                        intern_backend,
                        resolved_intern.provider.clone(),
                        resolved_intern.model.clone(),
                        Arc::clone(&cache),
                        trader_backend,
                        trader_params.clone(),
                        Arc::clone(&portfolio_provider),
                    ))
                }
                ArmKind::BuyAndHold => Box::new(BuyAndHold::new()),
                ArmKind::AlwaysLong => Box::new(AlwaysLong),
                ArmKind::AlwaysShort => Box::new(AlwaysShort),
                ArmKind::RandomDirection { seed } => Box::new(RandomDirection::new(seed)),
                ArmKind::RsiMeanReversion => Box::new(RsiMeanReversion::new()),
                ArmKind::MaCrossover { fast, slow } => Box::new(MaCrossover::new(fast, slow)),
                ArmKind::MacdMomentum => Box::new(MacdMomentum::new()),
            };
            Ok(ArmConfig {
                name: spec.name,
                strategy,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut runner = BacktestRunner::new(config, arm_configs)?;
    let result = runner.run(&snapshots, &bars, risk).await?;
    Ok(result)
}
```

Keep the old `run_ab_compare` function intact for one task — it's still called by the CLI in `commands/ab_compare.rs` and the swap happens in Task 12. Mark it deprecated with a doc comment:

```rust
/// **DEPRECATED**: use `run_ab_compare_v2` with a `ProviderRegistry`. Kept
/// in place for one commit to avoid breaking the CLI build between Task 11
/// and Task 12.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p xvision-eval`
Expected: PASS — existing tests + the new `registry_wiring::resolve_uses_registry_with_per_arm_override_when_present`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-eval/src/ab_compare.rs
git commit -m "feat(eval): run_ab_compare_v2 dispatches per-arm slots via ProviderRegistry"
```

---

### Task 12: Swap CLI `commands/ab_compare.rs` to `run_ab_compare_v2`

**Files:**
- Modify: `crates/xvision-cli/src/commands/ab_compare.rs`
- Modify: `crates/xvision-eval/src/ab_compare.rs` (delete deprecated `run_ab_compare`)

- [ ] **Step 1: Rewrite the CLI command**

Replace `crates/xvision-cli/src/commands/ab_compare.rs` body with:

```rust
//! `xvn ab-compare` — N-arm backtest A/B runner.
//!
//! Each `trader_arm` may carry inline `intern=<provider>/<model>` and
//! `trader=<provider>/<model>` overrides; otherwise the global CLI flags
//! supply the defaults via the ProviderRegistry.

use std::path::PathBuf;
use std::sync::Arc;

use xvision_core::config::{ProviderEntry, ProviderKind};
use xvision_core::market::MarketSnapshot;
use xvision_core::slot::SlotRef;
use xvision_core::trading::{AssetSymbol, PortfolioState};
use xvision_eval::ab_compare::{
    auto_suffix_arm_names, default_arms, parse_arm_spec, run_ab_compare_v2,
};
use xvision_eval::backtest::MarketBar;
use xvision_eval::baselines::PortfolioProvider;
use xvision_eval::harness::BacktestRunConfig;
use xvision_eval::provider_registry::ProviderRegistry;
use xvision_trader::TraderParams;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    setups: PathBuf,
    bars: PathBuf,
    arms: String,
    output: PathBuf,
    initial_nav_usd: f64,
    fee_bps: u32,
    step_hours: u32,
    horizon_hours: u32,
    asset: String,
    intern_provider: String,
    intern_model: String,
    trader_base_url: String,
    trader_model: String,
    trader_api_key_env: String,
) -> anyhow::Result<()> {
    let snapshots: Vec<MarketSnapshot> = serde_json::from_slice(&std::fs::read(&setups)?)?;
    let bars_vec: Vec<MarketBar> = serde_json::from_slice(&std::fs::read(&bars)?)?;
    let mut arm_specs: Vec<_> = if arms.trim().is_empty() {
        default_arms()
    } else {
        arms.split(',')
            .map(|s| parse_arm_spec(s.trim()))
            .collect::<anyhow::Result<Vec<_>>>()?
    };
    auto_suffix_arm_names(&mut arm_specs);

    let asset_sym = match asset.as_str() {
        "BTC" => AssetSymbol::Btc,
        "ETH" => AssetSymbol::Eth,
        "SOL" => AssetSymbol::Sol,
        other => anyhow::bail!("unknown asset: {other}"),
    };

    // Build the registry from config + CLI flag fallbacks.
    let workspace_root = std::env::current_dir()?;
    let runtime_cfg =
        xvision_core::config::load_runtime(&workspace_root.join("config/default.toml"))?;
    let mut rows = runtime_cfg.providers;

    // Synthesize a CLI-default-trader row if the trader_base_url+key isn't
    // already represented under any registered openai-compat provider.
    let cli_trader_kind = ProviderKind::OpenaiCompat;
    let cli_trader_provider_name = rows
        .iter()
        .find(|p| p.matches_triple(cli_trader_kind, &trader_base_url, &trader_api_key_env))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| {
            let synth_name = "_cli_default_trader".to_string();
            rows.push(ProviderEntry {
                name: synth_name.clone(),
                kind: cli_trader_kind,
                base_url: trader_base_url.clone(),
                api_key_env: trader_api_key_env.clone(),
            });
            synth_name
        });

    // Resolve the CLI-default-intern provider name. The auto-derived
    // `_default_intern` row already covers the [intern] block triple; CLI
    // flags `--intern --intern-model` override only the model id, with the
    // base_url/api_key_env carried over from the existing intern config.
    let cli_intern_kind: ProviderKind = match intern_provider.as_str() {
        "anthropic" => ProviderKind::Anthropic,
        "openai-compat" => ProviderKind::OpenaiCompat,
        other => anyhow::bail!("unknown intern provider: {other}"),
    };
    let cli_intern_provider_name = rows
        .iter()
        .find(|p| p.kind == cli_intern_kind)
        .map(|p| p.name.clone())
        .ok_or_else(|| anyhow::anyhow!(
            "no provider row matches CLI --intern={intern_provider}; \
             register one under [[providers]] in config/default.toml"
        ))?;

    let registry = Arc::new(ProviderRegistry::new(
        rows,
        SlotRef::new(cli_intern_provider_name, intern_model),
        SlotRef::new(cli_trader_provider_name, trader_model),
    ));

    let risk = xvision_harness::load_risk_layer(&workspace_root)?;
    let cfg = BacktestRunConfig {
        initial_nav_usd,
        fee_bps,
        slippage_atr_frac: 0.0,
        instrument: asset_sym,
        step_hours,
        horizon_hours,
        n_bootstrap_resamples: 1000,
        block_size: None,
    };

    let init_nav = initial_nav_usd;
    let portfolio_provider: PortfolioProvider = Arc::new(move || PortfolioState {
        equity_usd: init_nav,
        realized_pnl_today_usd: 0.0,
        day_index: 0,
        open_positions: Default::default(),
        as_of: chrono::Utc::now(),
    });

    println!(
        "running {} arm(s) over {} setup(s) / {} bar(s)…",
        arm_specs.len(),
        snapshots.len(),
        bars_vec.len()
    );
    let result = run_ab_compare_v2(
        snapshots,
        bars_vec,
        arm_specs,
        cfg,
        registry,
        TraderParams::default(),
        portfolio_provider,
        &risk,
    )
    .await?;

    std::fs::write(&output, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "wrote {} arm result(s) → {}",
        result.arms.len(),
        output.display()
    );
    Ok(())
}
```

- [ ] **Step 2: Delete the deprecated `run_ab_compare`**

In `crates/xvision-eval/src/ab_compare.rs`, remove the function marked `**DEPRECATED**` from Task 11 and rename `run_ab_compare_v2` → `run_ab_compare` (keep the rename to preserve the public API name). Update the call site in `commands/ab_compare.rs` to call `run_ab_compare` instead of `run_ab_compare_v2`.

- [ ] **Step 3: Build and run all tests**

Run: `cargo build -p xvision-cli && cargo test --workspace`
Expected: PASS — full workspace builds; all tests pass.

- [ ] **Step 4: Smoke test against mock fixtures (optional but recommended)**

If `data/probes/` already has a small setup+bars fixture, run:

```bash
cargo run -p xvision-cli -- ab-compare \
  --setups data/probes/<setups>.json \
  --bars data/probes/<bars>.json \
  --arms 'trader_arm:trader=openai/gpt-4o,trader_arm:trader=openai/gpt-4o-mini' \
  --output /tmp/ab-smoke.json
```

Expected: tracing emits two `arm dispatch` lines with `arm=trader_arm[gpt-4o]` and `arm=trader_arm[gpt-4o-mini]`. Both share the Intern Arc (`cache: 1 unique intern slot…` if the optional log-once line from spec §6.3 is wired; otherwise verifiable by Stage-1 call count). Skip this step if no fixture is checked in.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/ab_compare.rs crates/xvision-eval/src/ab_compare.rs
git commit -m "feat(cli): xvn ab-compare resolves per-arm slots via ProviderRegistry"
```

---

## Phase 4 — `xvn provider` subcommand + cache divergence test (Day 4)

### Task 13: `xvn provider list` + `xvn provider show`

**Files:**
- Create: `crates/xvision-cli/src/commands/provider.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Modify: `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Stub the subcommand**

Create `crates/xvision-cli/src/commands/provider.rs`:

```rust
//! `xvn provider …` — list / show / check / add / remove registered LLM
//! providers. Reads from / writes to `config/default.toml`.
//!
//! `add` and `remove` mutate the file in place via `toml_edit` to preserve
//! comments and formatting. `list` and `show` are read-only.

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct ProviderCmd {
    #[command(subcommand)]
    action: ProviderAction,
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    /// List all registered providers.
    List,
    /// Show one provider in full.
    Show {
        #[arg(long)]
        name: String,
    },
    /// Probe a provider for reachability.
    Check {
        #[arg(long)]
        name: String,
        /// Send a real /models request (costs nothing on most providers but
        /// burns a request quota slot). Default is a TCP-connect smoke.
        #[arg(long, default_value_t = false)]
        probe: bool,
    },
    /// Register a new provider in config/default.toml.
    Add {
        #[arg(long)]
        name: String,
        /// `anthropic` | `openai-compat` | `local-candle`.
        #[arg(long)]
        kind: String,
        #[arg(long)]
        base_url: String,
        /// Env var holding the API key (empty for no-auth endpoints).
        #[arg(long, default_value = "")]
        api_key_env: String,
    },
    /// Remove a provider by name. Refused if any slot references it.
    Remove {
        #[arg(long)]
        name: String,
    },
}

pub async fn run(cmd: ProviderCmd) -> anyhow::Result<()> {
    let config_path = std::env::current_dir()?.join("config/default.toml");
    match cmd.action {
        ProviderAction::List => list(&config_path),
        ProviderAction::Show { name } => show(&config_path, &name),
        ProviderAction::Check { name, probe } => check(&config_path, &name, probe).await,
        ProviderAction::Add {
            name,
            kind,
            base_url,
            api_key_env,
        } => add(&config_path, &name, &kind, &base_url, &api_key_env),
        ProviderAction::Remove { name } => remove(&config_path, &name),
    }
}

fn list(config_path: &std::path::Path) -> anyhow::Result<()> {
    let cfg = xvision_core::config::load_runtime(config_path)?;
    println!(
        "{:<18} {:<14} {:<42} {:<22} {}",
        "NAME", "KIND", "BASE_URL", "API_KEY_ENV", "KEY"
    );
    for p in &cfg.providers {
        let key_state = if p.api_key_env.is_empty() {
            "n/a".to_string()
        } else if std::env::var(&p.api_key_env).is_ok() {
            "● set".to_string()
        } else {
            "○ missing".to_string()
        };
        let kind = match p.kind {
            xvision_core::config::ProviderKind::Anthropic => "anthropic",
            xvision_core::config::ProviderKind::OpenaiCompat => "openai-compat",
            xvision_core::config::ProviderKind::LocalCandle => "local-candle",
        };
        let env_display = if p.api_key_env.is_empty() {
            "(none)".to_string()
        } else {
            p.api_key_env.clone()
        };
        let synth_marker = if p.name.starts_with('_') { "  (synthetic)" } else { "" };
        println!(
            "{:<18} {:<14} {:<42} {:<22} {}{}",
            p.name, kind, p.base_url, env_display, key_state, synth_marker
        );
    }
    Ok(())
}

fn show(config_path: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let cfg = xvision_core::config::load_runtime(config_path)?;
    let p = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| anyhow::anyhow!("provider `{name}` not found"))?;
    println!("{}", serde_json::to_string_pretty(p)?);
    if !p.api_key_env.is_empty() {
        let state = if std::env::var(&p.api_key_env).is_ok() {
            "set"
        } else {
            "missing"
        };
        println!("(env {} → {state})", p.api_key_env);
    }
    Ok(())
}

async fn check(
    _config_path: &std::path::Path,
    _name: &str,
    _probe: bool,
) -> anyhow::Result<()> {
    // Implemented in Task 15.
    anyhow::bail!("`xvn provider check` lands in Task 15")
}

fn add(
    _config_path: &std::path::Path,
    _name: &str,
    _kind: &str,
    _base_url: &str,
    _api_key_env: &str,
) -> anyhow::Result<()> {
    anyhow::bail!("`xvn provider add` lands in Task 14")
}

fn remove(_config_path: &std::path::Path, _name: &str) -> anyhow::Result<()> {
    anyhow::bail!("`xvn provider remove` lands in Task 14")
}
```

- [ ] **Step 2: Wire the module**

Add to `crates/xvision-cli/src/commands/mod.rs`:

```rust
pub mod provider;
```

Add to `crates/xvision-cli/src/lib.rs` `Command` enum:

```rust
/// Manage registered LLM providers in config/default.toml.
Provider(commands::provider::ProviderCmd),
```

And in the `match self.command` block:

```rust
Command::Provider(cmd) => commands::provider::run(cmd).await,
```

- [ ] **Step 3: Build and smoke-test**

Run: `cargo build -p xvision-cli && cargo run -p xvision-cli -- provider list`
Expected: prints a table including `anthropic`, `openai`, `ollama-local` rows. Each row's `KEY` column reflects whether the named env var is currently set.

Add a `mod tests` to `provider.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_returns_err_for_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, MIN_CONFIG).unwrap();
        let err = show(&path, "nope").unwrap_err();
        assert!(format!("{err:#}").contains("not found"));
    }

    const MIN_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
}
```

Add `tempfile = "3"` to `xvision-cli`'s `[dev-dependencies]` if not already present.

Run: `cargo test -p xvision-cli provider::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/provider.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs crates/xvision-cli/Cargo.toml
git commit -m "feat(cli): xvn provider list + show subcommands"
```

---

### Task 14: `xvn provider add` + `xvn provider remove` (in-place TOML mutation)

**Files:**
- Modify: `crates/xvision-cli/src/commands/provider.rs`
- Modify: `crates/xvision-cli/Cargo.toml` — add `toml_edit = "0.22"` to `[dependencies]`

- [ ] **Step 1: Write the failing tests**

Append to `provider.rs` `mod tests`:

```rust
#[test]
fn add_appends_provider_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, MIN_CONFIG).unwrap();
    add(&path, "openai", "openai-compat", "https://api.openai.com/v1", "OPENAI_API_KEY")
        .unwrap();
    let cfg = xvision_core::config::load_runtime(&path).unwrap();
    assert!(cfg.providers.iter().any(|p| p.name == "openai"));
}

#[test]
fn add_rejects_duplicate_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, MIN_CONFIG).unwrap();
    let err = add(&path, "anthropic", "anthropic", "https://x", "K").unwrap_err();
    assert!(format!("{err:#}").contains("already exists"));
}

#[test]
fn add_rejects_invalid_kind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, MIN_CONFIG).unwrap();
    let err = add(&path, "x", "BOGUS", "https://x", "K").unwrap_err();
    assert!(format!("{err:#}").contains("kind"));
}

#[test]
fn remove_drops_provider_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    let mut src = MIN_CONFIG.to_string();
    src.push_str(
        r#"
[[providers]]
name = "ephemeral"
kind = "openai-compat"
base_url = "https://x"
api_key_env = "K"
"#,
    );
    std::fs::write(&path, src).unwrap();
    remove(&path, "ephemeral").unwrap();
    let cfg = xvision_core::config::load_runtime(&path).unwrap();
    assert!(!cfg.providers.iter().any(|p| p.name == "ephemeral"));
}

#[test]
fn remove_refuses_when_intern_block_references_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.toml");
    std::fs::write(&path, MIN_CONFIG).unwrap();
    // The auto-derived `anthropic` provider matches [intern]'s triple, so
    // removing it would leave the intern config dangling.
    let err = remove(&path, "anthropic").unwrap_err();
    assert!(format!("{err:#}").contains("referenced by [intern]"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p xvision-cli provider::tests::add_ provider::tests::remove_`
Expected: FAIL — current `add`/`remove` are `bail!` stubs.

- [ ] **Step 3: Implement `add` and `remove`**

Replace `add` and `remove` in `provider.rs`:

```rust
fn add(
    config_path: &std::path::Path,
    name: &str,
    kind: &str,
    base_url: &str,
    api_key_env: &str,
) -> anyhow::Result<()> {
    use toml_edit::{value, ArrayOfTables, DocumentMut, Table};

    // Validate kind up front so the user sees a clear error.
    match kind {
        "anthropic" | "openai-compat" | "local-candle" => {}
        other => anyhow::bail!(
            "invalid kind `{other}`; must be one of: anthropic | openai-compat | local-candle"
        ),
    }
    if name.starts_with('_') {
        anyhow::bail!("provider names starting with '_' are reserved");
    }

    let raw = std::fs::read_to_string(config_path)?;
    let mut doc: DocumentMut = raw.parse()?;
    let providers = match doc.entry("providers").or_insert_with(|| {
        toml_edit::Item::ArrayOfTables(ArrayOfTables::new())
    }) {
        toml_edit::Item::ArrayOfTables(arr) => arr,
        _ => anyhow::bail!("[[providers]] is not an array of tables"),
    };
    if providers
        .iter()
        .any(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
    {
        anyhow::bail!("provider `{name}` already exists");
    }
    let mut row = Table::new();
    row.insert("name", value(name));
    row.insert("kind", value(kind));
    row.insert("base_url", value(base_url));
    row.insert("api_key_env", value(api_key_env));
    providers.push(row);

    std::fs::write(config_path, doc.to_string())?;
    // Validate by reloading.
    xvision_core::config::load_runtime(config_path)?;
    Ok(())
}

fn remove(config_path: &std::path::Path, name: &str) -> anyhow::Result<()> {
    use toml_edit::DocumentMut;

    let cfg = xvision_core::config::load_runtime(config_path)?;
    // If the [intern] block points at the same triple as this provider, refuse.
    let intern_kind: xvision_core::config::ProviderKind = cfg.intern.provider.into();
    if let Some(p) = cfg.providers.iter().find(|p| p.name == name) {
        if p.matches_triple(intern_kind, &cfg.intern.base_url, &cfg.intern.api_key_env) {
            anyhow::bail!(
                "cannot remove provider `{name}`: referenced by [intern] (workspace default Intern slot). \
                 Edit [intern] to point at a different provider first."
            );
        }
    } else {
        anyhow::bail!("provider `{name}` not found");
    }

    let raw = std::fs::read_to_string(config_path)?;
    let mut doc: DocumentMut = raw.parse()?;
    if let Some(toml_edit::Item::ArrayOfTables(arr)) = doc.get_mut("providers") {
        let before = arr.len();
        arr.retain(|t| t.get("name").and_then(|v| v.as_str()) != Some(name));
        if arr.len() == before {
            anyhow::bail!("provider `{name}` not found in TOML (race / synthetic row)");
        }
    } else {
        anyhow::bail!("no [[providers]] block in {}", config_path.display());
    }
    std::fs::write(config_path, doc.to_string())?;
    xvision_core::config::load_runtime(config_path)?;
    Ok(())
}
```

- [ ] **Step 4: Add `toml_edit` dependency**

Add to `crates/xvision-cli/Cargo.toml` `[dependencies]`:

```toml
toml_edit = "0.22"
```

- [ ] **Step 5: Run tests + smoke**

Run: `cargo test -p xvision-cli provider::tests`
Expected: PASS — all five new tests + the show test from Task 13.

Smoke:
```bash
cargo run -p xvision-cli -- provider add --name groq --kind openai-compat --base-url https://api.groq.com/openai/v1 --api-key-env GROQ_API_KEY
cargo run -p xvision-cli -- provider list
cargo run -p xvision-cli -- provider remove --name groq
```
Expected: row appears, then disappears. The `config/default.toml` file is updated in place — inspect with `git diff config/default.toml` to confirm the formatting was preserved.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-cli/src/commands/provider.rs crates/xvision-cli/Cargo.toml
git commit -m "feat(cli): xvn provider add/remove with toml_edit in-place mutation"
```

---

### Task 15: `xvn provider check` (TCP-connect + optional `--probe`)

**Files:**
- Modify: `crates/xvision-cli/src/commands/provider.rs`

- [ ] **Step 1: Implement `check`**

Replace the `check` stub in `provider.rs`:

```rust
async fn check(
    config_path: &std::path::Path,
    name: &str,
    probe: bool,
) -> anyhow::Result<()> {
    let cfg = xvision_core::config::load_runtime(config_path)?;
    let p = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| anyhow::anyhow!("provider `{name}` not found"))?;

    // Env-var presence check.
    if !p.api_key_env.is_empty() && std::env::var(&p.api_key_env).is_err() {
        println!("○ env {} not set", p.api_key_env);
    } else if !p.api_key_env.is_empty() {
        println!("● env {} set", p.api_key_env);
    }

    // TCP connect to base_url's host:port.
    let url = url_parse_minimal(&p.base_url)?;
    let stream = tokio::net::TcpStream::connect((url.host.as_str(), url.port)).await;
    match stream {
        Ok(_) => println!("● tcp {}:{} reachable", url.host, url.port),
        Err(e) => println!("○ tcp {}:{} {e}", url.host, url.port),
    }

    if probe {
        let client = reqwest::Client::new();
        let probe_url = if p.base_url.ends_with('/') {
            format!("{}models", p.base_url)
        } else {
            format!("{}/models", p.base_url)
        };
        let mut req = client.get(&probe_url);
        if !p.api_key_env.is_empty() {
            if let Ok(key) = std::env::var(&p.api_key_env) {
                req = req.header("Authorization", format!("Bearer {key}"));
            }
        }
        match req.send().await {
            Ok(resp) => println!("● GET {probe_url} → {}", resp.status()),
            Err(e) => println!("○ GET {probe_url} → {e}"),
        }
    }
    Ok(())
}

struct MinimalUrl {
    host: String,
    port: u16,
}

/// Tiny URL parser sufficient for `https://host[:port]/...` and
/// `http://host[:port]/...`. Avoids pulling in the `url` crate just for this.
fn url_parse_minimal(s: &str) -> anyhow::Result<MinimalUrl> {
    let (scheme, rest) = s
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("base_url missing scheme: {s}"))?;
    let host_port_path = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match host_port_path.split_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>().map_err(|e| anyhow::anyhow!("port parse: {e}"))?,
        ),
        None => (
            host_port_path.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    Ok(MinimalUrl { host, port })
}
```

- [ ] **Step 2: Add tokio + reqwest to xvision-cli if not already**

`xvision-cli` already depends on tokio (workspace) via other commands. Verify by checking `Cargo.toml`. Add to `[dependencies]` if missing:

```toml
tokio   = { workspace = true }
reqwest = { workspace = true }
```

- [ ] **Step 3: Add a parser test**

Append to `provider.rs` `mod tests`:

```rust
#[test]
fn url_parse_handles_https_default_port() {
    let u = url_parse_minimal("https://api.openai.com/v1").unwrap();
    assert_eq!(u.host, "api.openai.com");
    assert_eq!(u.port, 443);
}

#[test]
fn url_parse_handles_explicit_port() {
    let u = url_parse_minimal("http://localhost:11434/v1").unwrap();
    assert_eq!(u.host, "localhost");
    assert_eq!(u.port, 11434);
}

#[test]
fn url_parse_rejects_no_scheme() {
    assert!(url_parse_minimal("api.openai.com/v1").is_err());
}
```

- [ ] **Step 4: Run tests + smoke**

Run: `cargo test -p xvision-cli provider::tests::url_parse`
Expected: PASS — three tests.

Smoke (no real probe — TCP connect only):
```bash
cargo run -p xvision-cli -- provider check --name anthropic
```
Expected: prints `● env ANTHROPIC_API_KEY …` (set or missing) and `● tcp api.anthropic.com:443 reachable` (or `○ …` if offline).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/provider.rs crates/xvision-cli/Cargo.toml
git commit -m "feat(cli): xvn provider check (TCP-connect smoke + optional --probe)"
```

---

### Task 16: `cache_diverges_on_intern_model_change` test in `trader_arm.rs`

**Files:**
- Modify: `crates/xvision-eval/src/baselines/trader_arm.rs` (`mod tests`)

- [ ] **Step 1: Add the test**

Append to `crates/xvision-eval/src/baselines/trader_arm.rs` `mod tests`:

```rust
#[tokio::test]
async fn cache_diverges_on_intern_model_change() {
    let cache = Arc::new(BriefingCache::new());
    let snap = mk_snapshot();

    let key_haiku = CacheKey {
        cycle_id: snap.cycle_id,
        provider: "anthropic".into(),
        model: "claude-haiku-4-5".into(),
    };
    let key_opus = CacheKey {
        cycle_id: snap.cycle_id,
        provider: "anthropic".into(),
        model: "claude-opus-4-7".into(),
    };
    assert_eq!(key_haiku.cycle_id, key_opus.cycle_id);
    assert_ne!(
        format!("{:?}", key_haiku),
        format!("{:?}", key_opus),
        "key changes when intern model differs"
    );

    // Inserting under one key must not satisfy a lookup under the other.
    let intern = MockIntern;
    let briefing = intern
        .brief("p", snap.cycle_id, snap.asset, snap.regime, snap.horizon_hours)
        .await
        .unwrap();
    cache.insert(key_haiku.clone(), briefing.clone());
    assert!(cache.get(&key_haiku).is_some());
    assert!(
        cache.get(&key_opus).is_none(),
        "different intern model must miss the cache → Stage 1 re-runs"
    );
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p xvision-eval cache_diverges_on_intern_model_change`
Expected: PASS — confirms the existing `BriefingCache` already gives us per-Intern-model divergence (no code change needed; this is a regression-locking test for the spec §3.5 promise).

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-eval/src/baselines/trader_arm.rs
git commit -m "test(eval): lock BriefingCache divergence semantics on intern_model change"
```

---

## Phase 5 — UI design lock + migration note (Day 5)

These tasks edit `docs/design/ui-elements.md` (the canonical UI spec) and one CHANGELOG entry. No new code; no tests. Each task is one focused doc edit + commit.

### Task 17: Update `ui-elements.md` §13.1 to "Providers"

**Files:**
- Modify: `docs/design/ui-elements.md` §13.1 (~line 854)

- [ ] **Step 1: Replace the §13.1 LLM keys subsection**

Find:

```markdown
### 13.1 LLM keys
### 13.2 Brokers
…
(See v0.1 doc for field-level detail; no changes in v0.2.)
```

Replace `### 13.1 LLM keys` with:

```markdown
### 13.1 Providers

(Was "LLM keys" in v0.1 — promoted to a first-class registry. v0.1 single-key
state continues to work via auto-derivation; see spec
[`2026-05-10-llm-providers-and-per-arm-models-design.md`](../superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md) §1.3.)

| Element | Label / control |
|---|---|
| Page header | `Providers` · `+ Add provider` primary button |
| Table | columns: `Name`, `Kind`, `Base URL`, `API key env`, `Key`, `Used by`, `Actions` |
| Per row — Key chip | `● set` (green) when `std::env::var(api_key_env).is_ok()`, `○ missing` (amber) otherwise, `n/a` (grey) when the env name is empty |
| Per row — Used by | count + tooltip listing slot references (e.g. `2 slots: workspace default Intern, draft btc-momentum.trader`) |
| Per row — Actions | `Edit`, `Delete` (disabled with tooltip when `Used by > 0`), `Test` (calls `xvn provider check`) |
| Empty state | `No providers yet. Add Anthropic, OpenAI, or any OpenAI-compatible endpoint.` + three quick-link buttons (Anthropic / OpenAI / OpenRouter) carried over from the v0.1 first-run modal |
| Add modal | fields: `Name` (regex-validated `[a-z0-9-]+`), `Kind` (select: `anthropic` / `openai-compat` / `local-candle`), `Base URL`, `API key env` (with `Detect` ghost button trying common names like `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`), `Test connection` ghost. Submit calls `xvn provider add`. |
| Synthetic row marker | rows with name starting `_` (e.g. `_default_intern`, `_cli_default_trader`) render with a `synthetic` chip and a tooltip explaining they were auto-derived |

The first-run modal at `/setup` (§3.3) keeps its current shape (single key paste); the saved key materializes as a `[[providers]]` row, not a standalone `[intern]` block.
```

- [ ] **Step 2: Commit**

```bash
git add docs/design/ui-elements.md
git commit -m "docs(design): replace §13.1 LLM keys with Providers registry section"
```

---

### Task 18: Update `ui-elements.md` §4.2.2 — Inspector slot Provider select + cost-cue chips

**Files:**
- Modify: `docs/design/ui-elements.md` §4.2.2 (~line 381)

- [ ] **Step 1: Insert Provider field above Model**

Find the "Slot form" table in §4.2.2:

```markdown
| Field | Label | Control |
|---|---|---|
| Enabled | `Use this agent` | toggle (Trader required, can't disable) |
| Model class | `Model` | select |
```

Replace the `Model class` row with two rows:

```markdown
| Provider | `Provider` | select sourced from `/settings → Providers`, with `+ Add new…` last item that opens the add-provider modal inline |
| Model | `Model` | combobox (free-text + autocomplete suggestions per provider — Anthropic suggests `claude-*`, OpenAI suggests `gpt-*`, etc.) |
```

- [ ] **Step 2: Add cost-cue chip note**

After the "Slot form" table (and before the "Right pane — Live preview" heading), insert:

```markdown
**Cost cue chip** (informational, dismissable per session):

- Intern slot / Regime slot: `Changes here re-run Stage 1 for every setup ($$ per arm)`
- Trader slot: `Changes here are cheap — Stage 1 is reused across arms`

Quotes the BriefingCache rule from spec
[`2026-05-10-llm-providers-and-per-arm-models-design.md`](../superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md) §3.5.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design/ui-elements.md
git commit -m "docs(design): add Provider select + cost-cue chips to Inspector LLM slots"
```

---

### Task 19: Update `ui-elements.md` §5 row action menu + §2.3.3 lineage cue variant

**Files:**
- Modify: `docs/design/ui-elements.md` §5 (~line 469) and §2.3.3 (~line 240)

- [ ] **Step 1: Add `Fork with different model →` to the row action menu**

Find in §5:

```markdown
| Row action menu | `⋯` → `Open in Inspector`, `Duplicate`, `Fork` *(new — preserves parentage)*, `Run eval`, `Deploy paper`, `Delete` |
```

Replace with:

```markdown
| Row action menu | `⋯` → `Open in Inspector`, `Duplicate`, `Fork` *(preserves parentage)*, `Fork with different model →` *(focused fork — opens Inspector with Trader Provider+Model select pre-focused)*, `Run eval`, `Deploy paper`, `Delete` |
```

- [ ] **Step 2: Add model-fork lineage cue variant**

Find in §2.3.3:

```markdown
A single line, optional: `You've forked btc-momentum 4 times this week — see
lineage →`. Only renders when ≥3 sibling drafts exist from one root. Links to
the future lineage tree view (Move G, deferred). v1 wireframes can show this
as a stub link with `Coming soon` chip.
```

Append after that paragraph:

```markdown
**Model-fork variant.** When ≥3 of the sibling drafts differ from the root
*only* on the Trader or Intern slot model, the cue flips to:
`You've A/B-tested btc-momentum across 4 models this week — see leaderboard →`.
The link points at `/eval/compare?ids=<lineage_root>` filtered by parent.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design/ui-elements.md
git commit -m "docs(design): add Fork-with-different-model action + model-fork lineage cue"
```

---

### Task 20: Migration note + cli-non-surfaced.md update

**Files:**
- Modify: `docs/cli-non-surfaced.md`
- Create: `docs/migrations/2026-05-10-providers-config.md`

- [ ] **Step 1: Document `xvn provider` in cli-non-surfaced.md or wherever the surfaced-CLI doc lives**

Read `docs/cli-non-surfaced.md` to confirm scope. If it lists deliberately-unsurfaced commands, leave it alone; if it indexes the full CLI surface, add a `xvn provider` row with subcommand summary. The exact text depends on what's already there — be a faithful continuation of the file's existing format.

- [ ] **Step 2: Write the migration note**

Create `docs/migrations/2026-05-10-providers-config.md`:

```markdown
# 2026-05-10 — Providers registry in `config/default.toml`

## What changed

`config/default.toml` now declares a `[[providers]]` array. The existing
`[intern]` block keeps working — at load time, an auto-derived
`_default_intern` provider row is synthesized if no `[[providers]]` row
matches its `(provider, base_url, api_key_env)` triple.

## Do I need to do anything?

**No** — existing configs load unchanged. The new shape is purely additive.

## Why

To enable per-arm Intern + Trader model selection in `xvn ab-compare`, the
Inspector UI, and `Fork with different model →`. See spec:
[`docs/superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md`](../superpowers/specs/2026-05-10-llm-providers-and-per-arm-models-design.md).

## Optional cleanup

If you want to make the synthetic `_default_intern` row go away:

1. Add an explicit `[[providers]]` row that matches your `[intern]` triple:

   ```toml
   [[providers]]
   name        = "anthropic"
   kind        = "anthropic"
   base_url    = "https://api.anthropic.com"
   api_key_env = "ANTHROPIC_API_KEY"
   ```

2. Run `xvn provider list` to verify the synthetic row no longer appears.

The repo's `config/default.toml` already includes the explicit row in the v1
release commit.

## New CLI

```
xvn provider list
xvn provider show --name <name>
xvn provider check --name <name> [--probe]
xvn provider add --name <name> --kind <anthropic|openai-compat|local-candle> \
    --base-url <url> [--api-key-env <ENV>]
xvn provider remove --name <name>
```

## New `xvn ab-compare` arm-spec syntax

```
trader_arm                                     # unchanged: uses CLI defaults
trader_arm:trader=openai/gpt-4o                # override Trader slot only
trader_arm:intern=anthropic/claude-opus-4-7   # override Intern slot only
trader_arm:intern=…:trader=…                   # both
trader_arm:trader_model=gpt-4o-mini            # shorthand: keep default Trader provider, swap model
```

Auto-suffix gives each `trader_arm` a unique `BacktestResult` row name,
e.g. `trader_arm[gpt-4o]`, `trader_arm[claude-opus-4-7]`. Bare `trader_arm`
keeps its name.
```

- [ ] **Step 3: Commit**

```bash
git add docs/migrations/2026-05-10-providers-config.md docs/cli-non-surfaced.md
git commit -m "docs: migration note + cli-non-surfaced entry for provider registry"
```

---

## Self-review

- **Spec coverage:**
  - §1.1 in-scope items map to: providers vec (T1–T4), SlotRef + ArmSpec extension (T5–T7), per-arm wiring (T8–T11), `xvn provider` (T13–T15), BriefingCache test (T16), UI design lock (T17–T19), migration note (T20). ✅
  - §3.5 Cache divergence semantics → covered by T16. ✅
  - §6.2 `xvn provider` subcommand → T13–T15. ✅
  - §7 UI design lock → T17–T19. ✅
- **Placeholder scan:** No `TBD`/`TODO`/`fill in details`. The two `xvn provider check`/`add`/`remove` `bail!` stubs in T13 are intentional — they get implemented in the next two tasks, in code blocks, not as prose. ✅
- **Type consistency:** `ProviderEntry`, `ProviderKind`, `SlotRef`, `ArmKind::Trader { intern, trader }`, `ProviderRegistry::{intern_backend, trader_backend, default_intern, default_trader}`, `auto_suffix_arm_names`, `auto_derive_intern_provider_row`, `validate_unique_provider_names`, `matches_triple` — all consistent across tasks. `run_ab_compare_v2` is renamed back to `run_ab_compare` in T12 (and the deprecated original deleted). ✅
- **Open spec questions** that the plan defers (per spec §10): `xvn provider check --probe` default behavior (T15 chose TCP-connect default, real probe on `--probe`), Regime slot wiring (deliberately not added — schema doesn't accommodate it yet beyond `ArmKind::Trader`'s two slots; lock for v2). The `temperature/max_tokens/reasoning_effort` location question is out of scope for this plan — current behavior preserved (lives on `[intern]` / `TraderParams`).
