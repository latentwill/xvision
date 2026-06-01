# Typed Exit Codes (Plan 2b-followup) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Source:** the highest-leverage Printing Press recommendation from `docs/superpowers/research/2026-05-11-printing-press-review-xvn-cli.md` — agents reading `4 → not found` instead of grepping anyhow output.
> **Scope decision (2026-05-11):** Targeted to `xvn skill *`, `xvn strategy *`, and `xvn eval *` — the verbs an autooptimizer / dashboard / human agent calls every loop. Other verbs (fire-trade, venue, store, dashboard, eod, indicator, intern, trader, risk, provider, ab-compare, run-setup, show-*) keep returning anyhow → exit 1, with the new `From<anyhow::Error> for CliError` defaulting them to `XvnExit::Upstream`. They can opt in incrementally.

**Goal:** `xvn skill`, `xvn strategy`, and `xvn eval` return Printing-Press-style typed exit codes (0 success / 2 usage / 3 auth / 4 not found / 5 upstream / 7 conflict) so AI agents can self-correct without parsing error text. Adds `XvnExit` enum, `CliError` newtype with `?`-friendly conversion, and per-command exit-code wiring with integration tests asserting `process::exit_code()` per scenario.

**Architecture:** New `crates/xvision-cli/src/exit.rs` defines `XvnExit` (`#[repr(u8)]` enum) and `CliError { exit, source }`. `Cli::run()` becomes `Result<(), CliError>`; `From<anyhow::Error> for CliError` defaults uncategorized failures to `Upstream` so existing untyped commands keep working. `main.rs` returns `process::ExitCode` derived from `CliError::exit`. The targeted commands (`skill`, `strategy`, `eval`) return `CliResult<()>` and use a small extension trait `ResultExt::exit_with(XvnExit)` to attach categories at error sites — categorization happens at the boundary, engine error types stay untouched. The CLI consumes `xvision_engine::api::ApiError` directly (no longer flattened to anyhow) so `ApiError::NotFound → 4`, `ApiError::Validation → 2`, `ApiError::Conflict → 7` map mechanically without string-matching.

**Tech Stack:** Rust 2021. No new deps. Uses `std::process::ExitCode`, `anyhow`, `thiserror` already in workspace. Tests use `assert_cmd`-style: `Command::new(env!("CARGO_BIN_EXE_xvn")).output().status.code()`.

**Out of scope:**
- Wiring exit codes into untyped commands (fire-trade, venue, store, …). They keep returning `anyhow::Result<()>` and silently coerce to `XvnExit::Upstream`. Per-command opt-in is its own follow-up.
- Refactoring engine error types. `xvision-engine::api::ApiError` is already strongly typed; engine code beyond that stays as-is.
- Renaming binaries to `<api>-pp-cli` / `<api>-pp-mcp` (PP convention). Conflicts with our identity; documented as kept-our-way in the PP review.
- Auto-JSON when piped (separate small follow-up).
- `--dry-run` on mutations (separate small follow-up).

---

## File structure

```
crates/xvision-cli/
├── src/
│   ├── exit.rs                    # NEW — XvnExit + CliError + ResultExt
│   ├── lib.rs                     # MODIFIED — Cli::run signature + dispatch arms
│   ├── main.rs                    # MODIFIED — return process::ExitCode
│   ├── commands/
│   │   ├── skill.rs               # MODIFIED — return CliResult, attach exit codes
│   │   ├── strategy.rs            # MODIFIED — return CliResult, attach exit codes
│   │   └── eval.rs                # MODIFIED — return CliResult, attach exit codes
│   └── …
└── tests/
    ├── exit_codes_skill.rs        # NEW — 6 tests asserting status.code() per scenario
    ├── exit_codes_strategy.rs     # NEW — 6 tests
    └── exit_codes_eval.rs         # NEW — 6 tests
```

Plus docs:
- `MANUAL.md` — new "Exit codes" section under "AI agent drives xvn"
- `crates/xvision-skills/README.md` — link to MANUAL exit-code section
- `crates/xvision-engine/README.md` — link to MANUAL exit-code section

Total: 1 new crate file, 6 modified files, 3 new test files, 3 doc updates. ~9 commits.

---

## Phase A — Scaffold `XvnExit` + `CliError` + main wiring

### Task 1: Create `exit.rs` module + unit tests for the wrapper

**Files:**
- Create: `crates/xvision-cli/src/exit.rs`

- [ ] **Step 1: Write the module**

```rust
//! Typed exit codes following the Printing Press convention.
//!
//! Agents calling `xvn` programmatically can dispatch on the exit code
//! without parsing error text:
//!
//! ```text
//!   0  Success     command completed
//!   2  Usage       caller-fixable: bad flag, malformed input, unknown enum variant
//!   3  Auth        missing / invalid credential (e.g. ANTHROPIC_API_KEY)
//!   4  NotFound    referenced resource does not exist (strategy id, skill name, run id)
//!   5  Upstream    LLM API / broker / network / file system / database error
//!   7  Conflict    state collision (e.g. attaching a skill to an empty slot)
//! ```
//!
//! Commands carry the category through `CliError`. `From<anyhow::Error>`
//! defaults unattributed failures to `Upstream` so untyped commands keep
//! compiling. Use the `ResultExt::exit_with` helper at error sites to
//! attach a category to a typed `Result`.

use std::process::ExitCode;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XvnExit {
    Success  = 0,
    Usage    = 2,
    Auth     = 3,
    NotFound = 4,
    Upstream = 5,
    Conflict = 7,
}

impl From<XvnExit> for ExitCode {
    fn from(e: XvnExit) -> ExitCode {
        ExitCode::from(e as u8)
    }
}

#[derive(Debug)]
pub struct CliError {
    pub exit: XvnExit,
    pub source: anyhow::Error,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.source)
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.source()
    }
}

impl CliError {
    pub fn usage(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Usage, source: e.into() }
    }
    pub fn auth(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Auth, source: e.into() }
    }
    pub fn not_found(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::NotFound, source: e.into() }
    }
    pub fn upstream(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Upstream, source: e.into() }
    }
    pub fn conflict(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Conflict, source: e.into() }
    }
}

/// Default categorization for untyped failures bubbling up through `?`.
/// Without this, every untyped command's anyhow error would have no exit
/// category. Defaulting to Upstream is the conservative choice — it tells
/// the agent "external system failure, retry might help" rather than the
/// stronger "not found" or "auth", which would mislead retry logic.
impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        Self { exit: XvnExit::Upstream, source: e }
    }
}

pub type CliResult<T> = Result<T, CliError>;

/// Extension trait letting commands attach an exit category at the error
/// site:
///
/// ```ignore
/// let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
/// ```
pub trait ResultExt<T> {
    fn exit_with(self, code: XvnExit) -> CliResult<T>;
}

impl<T, E: Into<anyhow::Error>> ResultExt<T> for Result<T, E> {
    fn exit_with(self, code: XvnExit) -> CliResult<T> {
        self.map_err(|e| CliError { exit: code, source: e.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xvn_exit_code_values() {
        assert_eq!(XvnExit::Success as u8,  0);
        assert_eq!(XvnExit::Usage as u8,    2);
        assert_eq!(XvnExit::Auth as u8,     3);
        assert_eq!(XvnExit::NotFound as u8, 4);
        assert_eq!(XvnExit::Upstream as u8, 5);
        assert_eq!(XvnExit::Conflict as u8, 7);
    }

    #[test]
    fn anyhow_to_cli_error_defaults_to_upstream() {
        let e: anyhow::Error = anyhow::anyhow!("boom");
        let c: CliError = e.into();
        assert_eq!(c.exit, XvnExit::Upstream);
    }

    #[test]
    fn result_ext_attaches_category() {
        fn fails() -> anyhow::Result<()> {
            Err(anyhow::anyhow!("missing"))
        }
        let r: CliResult<()> = fails().exit_with(XvnExit::NotFound);
        let err = r.unwrap_err();
        assert_eq!(err.exit, XvnExit::NotFound);
        assert!(err.source.to_string().contains("missing"));
    }

    #[test]
    fn cli_error_helpers_set_correct_category() {
        assert_eq!(CliError::usage(anyhow::anyhow!("x")).exit,    XvnExit::Usage);
        assert_eq!(CliError::auth(anyhow::anyhow!("x")).exit,     XvnExit::Auth);
        assert_eq!(CliError::not_found(anyhow::anyhow!("x")).exit, XvnExit::NotFound);
        assert_eq!(CliError::upstream(anyhow::anyhow!("x")).exit, XvnExit::Upstream);
        assert_eq!(CliError::conflict(anyhow::anyhow!("x")).exit, XvnExit::Conflict);
    }
}
```

- [ ] **Step 2: Build + test the module**

Run: `cargo test -p xvision-cli exit::tests --lib`
Expected: 4 tests pass.

The module is not wired into anything yet, but `pub mod exit;` lands next.

- [ ] **Step 3: Wire `pub mod exit;` into `lib.rs`**

In `crates/xvision-cli/src/lib.rs`, near `pub mod commands;`, add:

```rust
pub mod commands;
pub mod exit;
```

- [ ] **Step 4: Build to confirm**

Run: `cargo build -p xvision-cli`
Expected: clean build.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/exit.rs crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): XvnExit + CliError scaffold (Plan 2b-followup Task 1)"
```

---

### Task 2: Convert `Cli::run` to `CliResult<()>` + main returns `ExitCode`

**Files:**
- Modify: `crates/xvision-cli/src/lib.rs` (signature on `impl Cli { pub async fn run(self) -> ... }`)
- Modify: `crates/xvision-cli/src/main.rs`

`From<anyhow::Error> for CliError` makes this a near-mechanical rename — the `?` operator on every `commands::*::run(cmd).await` arm coerces silently. After this task, every command's behavior is exit code 0 (success) or 5 (upstream default for any error), with nothing else categorized yet. Targeted commands get categorized in Phases B/C/D.

- [ ] **Step 1: Change `Cli::run` signature**

In `crates/xvision-cli/src/lib.rs`, find the `impl Cli { pub async fn run(self) -> anyhow::Result<()>` block and change to:

```rust
impl Cli {
    pub async fn run(self) -> Result<(), crate::exit::CliError> {
        match self.command {
            // … all match arms keep their `commands::foo::run(...).await` —
            // anyhow::Result auto-coerces to CliResult via `?` thanks to
            // `From<anyhow::Error> for CliError`. The match arms themselves
            // do not change.
            // <existing arms unchanged>
        }
    }
}
```

The match-arm bodies don't need rewriting — the `?` operator on each arm now coerces `anyhow::Error` → `CliError` automatically. The whole edit is the function signature line plus the closing `?` on each call already present. (Some arms `return X.await`; those stay because `Result<(), anyhow::Error>` coerces to `Result<(), CliError>` via `?` only when `?` is used — for a bare return, change `commands::foo::run(c).await` to `commands::foo::run(c).await?` and add `Ok(())` at the end of each match arm if needed.)

Concretely: wherever an arm reads `Command::Foo(c) => commands::foo::run(c).await,` change to `Command::Foo(c) => commands::foo::run(c).await.map_err(Into::into),`.

- [ ] **Step 2: Update `main.rs`**

```rust
//! xvn — XVISION CLI entry point.

use std::process::ExitCode;

use clap::Parser;
use xvision_cli::{exit::XvnExit, Cli};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.run().await {
        Ok(()) => XvnExit::Success.into(),
        Err(e) => {
            eprintln!("{e}");
            e.exit.into()
        }
    }
}
```

- [ ] **Step 3: Build the workspace**

Run: `cargo build -p xvision-cli`
Expected: clean build. If a dispatch arm doesn't compile, change its body to `commands::foo::run(c).await.map_err(Into::into)`.

- [ ] **Step 4: Run the existing CLI test suite — all passes mean we haven't regressed behavior**

Run: `cargo test -p xvision-cli`
Expected: all existing tests pass (skill_cli + strategy_cli unchanged).

- [ ] **Step 5: Manual smoke — verify `--help` still returns 0, unknown command still returns 2 (clap usage)**

Run:
```bash
cargo run -q -p xvision-cli -- --help; echo "exit=$?"
cargo run -q -p xvision-cli -- bogus-command 2>/dev/null; echo "exit=$?"
```
Expected:
```
exit=0
exit=2
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-cli/src/lib.rs crates/xvision-cli/src/main.rs
git commit -m "feat(cli): main returns ExitCode; Cli::run returns CliResult (Plan 2b-followup Task 2)"
```

---

## Phase B — Wire `xvn skill` exit codes

### Task 3: Add typed exit codes to `xvn skill {new | ls | attach}`

**Files:**
- Modify: `crates/xvision-cli/src/commands/skill.rs`

Categorization plan for each verb:

| Verb | Failure mode | Exit |
|---|---|---|
| `skill new --from-file <path>` | file unreadable | 2 (Usage — caller's path) |
| `skill new` | parse error (malformed frontmatter, missing required field) | 2 (Usage — caller's input) |
| `skill new` | disk write failure | 5 (Upstream) |
| `skill ls` | listing succeeds but is empty | 0 |
| `skill ls` | disk failure | 5 (Upstream) |
| `skill attach <id>` | strategy not found | 4 (NotFound) |
| `skill attach … --skill X` | skill not found | 4 (NotFound) |
| `skill attach … --slot bogus` | unknown slot role | 2 (Usage — typo) |
| `skill attach … --slot regime` (slot empty) | attaching to empty slot | 7 (Conflict — bundle/intent collision) |
| `skill attach` | bundle save fails | 5 (Upstream) |

- [ ] **Step 1: Rewrite `commands/skill.rs` to return `CliResult<()>`**

Update the imports + signatures + helpers:

```rust
//! `xvn skill ...` — author and attach OSShip-style markdown skills.

use std::env;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use xvision_engine::bundle::store::{BundleStore, FilesystemStore};
use xvision_skills::attach::attach_skill_to_agent;
use xvision_skills::parse;
use xvision_skills::store::{FilesystemSkillStore, SkillStore};

use crate::exit::{CliError, CliResult, ResultExt, XvnExit};

// (SkillCmd / SkillAction unchanged — keep the existing struct + enum.)

pub async fn run(cmd: SkillCmd) -> CliResult<()> {
    match cmd.action {
        SkillAction::New { from_file } => new(from_file).await,
        SkillAction::Ls => ls().await,
        SkillAction::Attach { agent_id, slot, skill } => {
            attach(&agent_id, &slot, &skill).await
        }
    }
}

fn xvn_home() -> PathBuf {
    if let Ok(p) = env::var("XVN_HOME") {
        return PathBuf::from(p);
    }
    dirs::home_dir().expect("$HOME").join(".xvn")
}

fn skill_store() -> FilesystemSkillStore {
    FilesystemSkillStore::new(xvn_home().join("skills"))
}
fn strategy_store() -> FilesystemStore {
    FilesystemStore::new(xvn_home().join("strategies"))
}

async fn new(from_file: PathBuf) -> CliResult<()> {
    let markdown = tokio::fs::read_to_string(&from_file)
        .await
        .exit_with(XvnExit::Usage)?; // file path came from the caller
    let parsed = parse(&markdown).exit_with(XvnExit::Usage)?; // input is caller's
    skill_store()
        .save(&parsed.name, &markdown)
        .await
        .exit_with(XvnExit::Upstream)?; // disk write
    println!("{}", parsed.name);
    Ok(())
}

async fn ls() -> CliResult<()> {
    for name in skill_store().list().await.exit_with(XvnExit::Upstream)? {
        println!("{name}");
    }
    Ok(())
}

async fn attach(agent_id: &str, slot: &str, skill_name: &str) -> CliResult<()> {
    let strategies = strategy_store();
    let mut bundle = strategies
        .load(agent_id)
        .await
        .exit_with(XvnExit::NotFound)?; // missing strategy id
    let skill = skill_store()
        .load(skill_name)
        .await
        .exit_with(XvnExit::NotFound)?; // missing skill name

    // attach_skill_to_agent returns anyhow::Error with two distinct messages:
    // "unknown slot role: ..." (caller typo → Usage)
    // "slot 'X' is empty — fill it before attaching" (state conflict → Conflict)
    if let Err(e) = attach_skill_to_agent(&mut bundle, slot, &skill) {
        let msg = e.to_string();
        let exit = if msg.contains("unknown slot role") {
            XvnExit::Usage
        } else {
            XvnExit::Conflict
        };
        return Err(CliError { exit, source: e });
    }

    strategies
        .save(&bundle)
        .await
        .exit_with(XvnExit::Upstream)?;
    println!("attached {skill_name} → {agent_id}#{slot}");
    Ok(())
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p xvision-cli`
Expected: clean build.

- [ ] **Step 3: Re-run existing skill_cli tests — they still assert success/error string contents but not exit codes; should still pass**

Run: `cargo test -p xvision-cli --test skill_cli`
Expected: 3/3 pass (unchanged from Plan 2b).

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/skill.rs
git commit -m "feat(cli): xvn skill returns typed exit codes (Plan 2b-followup Task 3)"
```

---

### Task 4: Integration test — exit codes for `xvn skill`

**Files:**
- Create: `crates/xvision-cli/tests/exit_codes_skill.rs`

- [ ] **Step 1: Write the test file**

```rust
//! Verify `xvn skill *` returns the expected XvnExit code per scenario.
//! Reads status.code() from the spawned subprocess; doesn't import the
//! XvnExit enum (the contract under test is the *number* on the wire).

use std::process::Command;
use tempfile::tempdir;

const FIXTURE: &str =
    include_str!("../../xvision-skills/tests/fixtures/crypto-trader-base.md");

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

#[test]
fn skill_new_succeeds_returns_0() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("crypto-trader-base.md");
    std::fs::write(&p, FIXTURE).unwrap();
    let out = xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    assert_eq!(code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_new_missing_file_returns_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["skill", "new", "--from-file", "/tmp/does-not-exist.md"],
        dir.path(),
    );
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_new_malformed_returns_2_usage() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("bad.md");
    std::fs::write(&p, "no frontmatter").unwrap();
    let out = xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_unknown_strategy_returns_4_not_found() {
    let dir = tempdir().unwrap();
    // register a skill so the skill load doesn't fail first
    let p = dir.path().join("s.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["skill", "attach", "no-such-strategy",
          "--slot", "trader", "--skill", "crypto-trader-base"],
        dir.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_unknown_skill_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["skill", "attach", &id, "--slot", "trader", "--skill", "no-such-skill"],
        dir.path(),
    );
    assert_eq!(code(&out), 4, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_unknown_slot_returns_2_usage() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("s.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["skill", "attach", &id, "--slot", "bogus", "--skill", "crypto-trader-base"],
        dir.path(),
    );
    assert_eq!(code(&out), 2, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn skill_attach_empty_slot_returns_7_conflict() {
    // mean_reversion template fills only the trader slot — regime + intern
    // are None. Attaching to regime should hit "slot is empty".
    let dir = tempdir().unwrap();
    let p = dir.path().join("s.md");
    std::fs::write(&p, FIXTURE).unwrap();
    xvn(&["skill", "new", "--from-file", p.to_str().unwrap()], dir.path());
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["skill", "attach", &id, "--slot", "regime", "--skill", "crypto-trader-base"],
        dir.path(),
    );
    assert_eq!(code(&out), 7, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}
```

Verify before writing: confirm `mean_reversion`'s draft has `regime_slot: None`. Spot-check via:
```bash
grep -A20 "mean_reversion" crates/xvision-engine/src/templates/registry.rs | head -40
```
If `mean_reversion` happens to fill regime in a future change, swap the empty-slot test to `intern`.

- [ ] **Step 2: Run tests**

Run: `cargo test -p xvision-cli --test exit_codes_skill`
Expected: 7/7 pass.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/tests/exit_codes_skill.rs
git commit -m "test(cli): exit-code coverage for xvn skill (Plan 2b-followup Task 4)"
```

---

## Phase C — Wire `xvn strategy` exit codes

### Task 5: Add typed exit codes to `xvn strategy {new | validate | ls | show | templates | run}`

**Files:**
- Modify: `crates/xvision-cli/src/commands/strategy.rs`

Categorization plan:

| Verb | Failure | Exit |
|---|---|---|
| `strategy templates` | only succeeds | 0 |
| `strategy ls` | listing succeeds (possibly empty) | 0 |
| `strategy ls` | disk failure | 5 |
| `strategy new` | unknown template | 2 (Usage — caller typo) |
| `strategy new` | bundle validation fails | 2 (Usage — caller's choice of name/creator) |
| `strategy new` | disk write fails | 5 |
| `strategy validate <id>` | bundle not found | 4 |
| `strategy validate <id>` | validation errors | 2 |
| `strategy show <id>` | not found | 4 |
| `strategy run <id>` | bundle not found | 4 |
| `strategy run <id>` | ANTHROPIC_API_KEY missing (real, not --mock) | 3 |
| `strategy run <id>` | LLM dispatch fails (network / 5xx) | 5 |
| `strategy run <id>` | empty asset_universe | 2 (corrupt input) |

- [ ] **Step 1: Add imports + change return type of `pub async fn run`**

In `crates/xvision-cli/src/commands/strategy.rs`, near the other use lines:

```rust
use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
```

Change:
```rust
pub async fn run(cmd: StrategyCmd) -> anyhow::Result<()> { … }
```
to:
```rust
pub async fn run(cmd: StrategyCmd) -> CliResult<()> {
    match cmd.action {
        StrategyAction::New { template, name, creator } => new(&template, &name, creator).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls => ls().await,
        StrategyAction::Show { id } => show(&id).await,
        StrategyAction::Templates => templates().await,
        StrategyAction::Run { id, fixture, decisions, mock } => {
            run_inline(&id, &fixture, decisions, mock).await
        }
    }
}
```

(Bodies of the helpers below.)

- [ ] **Step 2: Categorize the helpers**

Update each helper's signature + error sites. Keep behavior; only add `.exit_with(...)` and small explicit categorization:

```rust
async fn new(template: &str, name: &str, creator: Option<String>) -> CliResult<()> {
    let tpl = registry::get(template).ok_or_else(|| {
        CliError::usage(anyhow::anyhow!(
            "unknown template '{template}' — try `xvn strategy templates`"
        ))
    })?;
    let id = Ulid::new().to_string();
    let creator = creator
        .or_else(|| env::var("XVN_CREATOR").ok())
        .unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), name.to_string(), creator);
    validate_bundle(&draft).exit_with(XvnExit::Usage)?;
    store().save(&draft).await.exit_with(XvnExit::Upstream)?;
    println!("{id}");
    Ok(())
}

async fn validate(id: &str) -> CliResult<()> {
    let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
    validate_bundle(&bundle).exit_with(XvnExit::Usage)?;
    println!("ok");
    Ok(())
}

async fn ls() -> CliResult<()> {
    let ids = store().list().await.exit_with(XvnExit::Upstream)?;
    for id in ids { println!("{id}"); }
    Ok(())
}

async fn show(id: &str) -> CliResult<()> {
    let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
    let json = serde_json::to_string_pretty(&bundle).exit_with(XvnExit::Upstream)?;
    println!("{json}");
    Ok(())
}

async fn templates() -> CliResult<()> {
    let names = registry::list_template_names();
    for name in names {
        if let Some(tpl) = registry::get(&name) {
            println!("{:<20} {}", name, tpl.display_name());
        }
    }
    Ok(())
}
```

For `run_inline`, update each fallible step:

```rust
async fn run_inline(id: &str, fixture: &str, decisions: u32, mock: bool) -> CliResult<()> {
    let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
    let est = estimate_pipeline_tokens(&bundle, decisions as u64);
    println!("estimate: input={} output={} total={} (decisions={})",
             est.input, est.output, est.total, decisions);

    let dispatch: Arc<dyn LlmDispatch> = if mock {
        Arc::new(MockDispatch::echo(
            r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#,
        ))
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            CliError::auth(anyhow::anyhow!("set ANTHROPIC_API_KEY or pass --mock"))
        })?;
        Arc::new(AnthropicDispatch::new(key))
    };
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let asset = bundle.manifest.asset_universe.first().cloned().ok_or_else(|| {
        CliError::usage(anyhow::anyhow!("bundle has empty asset_universe"))
    })?;

    // … the per-decision pipeline loop. For each `?` on
    // `tool.invoke(...)` / `run_pipeline(...)`, append `.exit_with(XvnExit::Upstream)`.
    // Keep the existing loop body, only adding the categorization at the
    // ? sites.
    // … rest of the function unchanged, with `.exit_with(XvnExit::Upstream)?`
    //     on each invoke / pipeline call.
    Ok(())
}
```

The mechanical change for `run_inline`'s body is: every `?` on a fallible call inside the loop becomes `.exit_with(XvnExit::Upstream)?`. Read the current function and add the suffix at each error site. Don't restructure the loop.

- [ ] **Step 3: Build**

Run: `cargo build -p xvision-cli`
Expected: clean build.

- [ ] **Step 4: Existing strategy_cli tests still pass**

Run: `cargo test -p xvision-cli --test strategy_cli`
Expected: existing tests unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/strategy.rs
git commit -m "feat(cli): xvn strategy returns typed exit codes (Plan 2b-followup Task 5)"
```

---

### Task 6: Integration test — exit codes for `xvn strategy`

**Files:**
- Create: `crates/xvision-cli/tests/exit_codes_strategy.rs`

- [ ] **Step 1: Write the test file**

```rust
//! Verify `xvn strategy *` returns the expected XvnExit code per scenario.

use std::process::Command;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

#[test]
fn strategy_templates_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn strategy_ls_empty_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "ls"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn strategy_new_unknown_template_returns_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "new", "--template", "no-such-template", "--name", "x"],
        dir.path(),
    );
    assert_eq!(code(&out), 2);
}

#[test]
fn strategy_show_unknown_id_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"], dir.path());
    assert_eq!(code(&out), 4);
}

#[test]
fn strategy_validate_unknown_id_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "validate", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"], dir.path());
    assert_eq!(code(&out), 4);
}

#[test]
fn strategy_run_missing_anthropic_key_returns_3_auth() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["strategy", "new", "--template", "mean_reversion", "--name", "x"],
        dir.path(),
    );
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // Force ANTHROPIC_API_KEY unset — must use a Command that explicitly
    // removes the env var, since the parent process may have it set.
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["strategy", "run", &id, "--fixture", "test-fixture-btc-2024-01", "--decisions", "1"])
        .env("XVN_HOME", dir.path())
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .expect("xvn invocation");
    assert_eq!(code(&out), 3, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p xvision-cli --test exit_codes_strategy`
Expected: 6/6 pass.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/tests/exit_codes_strategy.rs
git commit -m "test(cli): exit-code coverage for xvn strategy (Plan 2b-followup Task 6)"
```

---

## Phase D — Wire `xvn eval` exit codes

### Task 7: Map `ApiError` variants to exit codes; update `eval.rs`

**Files:**
- Modify: `crates/xvision-cli/src/commands/eval.rs`

The eval CLI today flattens engine errors with `.map_err(|e| anyhow::anyhow!("eval get: {e}"))?`, which loses the typed `ApiError`. We need to preserve it and categorize.

Categorization plan:

| Verb | Failure | Exit |
|---|---|---|
| `eval list` | bad `--status` value | 2 (Usage) |
| `eval list` | listing succeeds | 0 |
| `eval show <id>` | run not found (`ApiError::NotFound`) | 4 |
| `eval show <id>` | other engine error | 5 |
| `eval scenarios` | engine error | 5 |
| `eval compare <ids>` | < 2 ids passed (clap rejects via `num_args=2..`) | 2 (clap default) |
| `eval compare <ids>` | run not found | 4 |
| `eval compare <ids>` | validation error from engine | 2 |
| `eval run` | strategy not found | 4 |
| `eval run` | scenario not found | 4 |
| `eval run` | bad `--mode` value | 2 |
| `eval run` | broker / dispatch error | 5 |
| `eval attest <id>` | run not found | 4 |
| `eval attest <id>` | run not yet completed (`Conflict`) | 7 |

- [ ] **Step 1: Add a small categorizer for `ApiError`**

In `crates/xvision-cli/src/commands/eval.rs`, add near the top:

```rust
use crate::exit::{CliError, CliResult, ResultExt, XvnExit};
use xvision_engine::api::ApiError;

/// Map an engine ApiError to our exit-code-bearing CliError. Variants
/// carry meaning that's worth preserving on the wire, so we don't fall
/// back to the default Upstream coercion.
fn api_to_cli(prefix: &str, e: ApiError) -> CliError {
    let exit = match &e {
        ApiError::NotFound(_)   => XvnExit::NotFound,
        ApiError::Validation(_) => XvnExit::Usage,
        ApiError::Conflict(_)   => XvnExit::Conflict,
        ApiError::Internal(_)
        | ApiError::Db(_)
        | ApiError::Other(_)    => XvnExit::Upstream,
    };
    CliError {
        exit,
        source: anyhow::anyhow!("{prefix}: {e}"),
    }
}
```

- [ ] **Step 2: Replace the flattening pattern across helpers**

Change each `pub async fn run(cmd: EvalCmd) -> Result<()>` and helpers to return `CliResult<()>`. Replace the existing pattern:
```rust
let run = eval::get(&ctx, &args.run_id)
    .await
    .map_err(|e| anyhow::anyhow!("eval get: {e}"))?;
```
with:
```rust
let run = eval::get(&ctx, &args.run_id)
    .await
    .map_err(|e| api_to_cli("eval get", e))?;
```

Apply the same pattern to `eval::run`, `eval::list`, `eval::compare`, `eval::scenarios`, `eval::attest`. For non-engine error sites (`open_ctx`, `parse_status`, `parse_mode`, `serde_json::to_string_pretty`), use `.exit_with(XvnExit::Usage)?` for parser/path errors and `.exit_with(XvnExit::Upstream)?` for serialization/I/O.

Specific sites to update:
- `parse_mode(s)` returns `anyhow::Result<RunMode>` → caller already uses `?`. Add `.exit_with(XvnExit::Usage)` at the call site in `run_run`.
- `parse_status(s)` same → `.exit_with(XvnExit::Usage)` at call sites in `run_list`.
- `open_ctx(args.xvn_home.clone()).await` → `.exit_with(XvnExit::Upstream)` (sqlite open / migration).
- All `serde_json::to_string_pretty` → `.exit_with(XvnExit::Upstream)`.

- [ ] **Step 3: Build**

Run: `cargo build -p xvision-cli`
Expected: clean build.

- [ ] **Step 4: Existing eval tests still pass**

Run: `cargo test -p xvision-cli eval 2>&1 | tail -10`
Expected: existing tests unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/eval.rs
git commit -m "feat(cli): xvn eval returns typed exit codes (Plan 2b-followup Task 7)"
```

---

### Task 8: Integration test — exit codes for `xvn eval`

**Files:**
- Create: `crates/xvision-cli/tests/exit_codes_eval.rs`

- [ ] **Step 1: Write the test file**

```rust
//! Verify `xvn eval *` returns the expected XvnExit code per scenario.
//! These tests exercise the verbs that don't need broker / dispatch
//! construction (list, show, scenarios, compare). `eval run` and
//! `eval attest` are deferred — they need richer fixture setup and
//! their own integration test file.

use std::process::Command;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

#[test]
fn eval_scenarios_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "scenarios"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn eval_list_empty_returns_0() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "list"], dir.path());
    assert_eq!(code(&out), 0);
}

#[test]
fn eval_list_bad_status_returns_2_usage() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "list", "--status", "bogus-status"], dir.path());
    assert_eq!(code(&out), 2);
}

#[test]
fn eval_show_unknown_run_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZ"], dir.path());
    assert_eq!(code(&out), 4);
}

#[test]
fn eval_compare_single_id_returns_2_clap_usage() {
    // num_args=2.. — clap rejects with exit 2 before reaching engine.
    let dir = tempdir().unwrap();
    let out = xvn(&["eval", "compare", "only-one-id"], dir.path());
    assert_eq!(code(&out), 2);
}

#[test]
fn eval_compare_two_unknown_ids_returns_4_not_found() {
    let dir = tempdir().unwrap();
    let out = xvn(
        &["eval", "compare",
          "01ZZZZZZZZZZZZZZZZZZZZZZZZ", "02ZZZZZZZZZZZZZZZZZZZZZZZZ"],
        dir.path(),
    );
    assert_eq!(code(&out), 4);
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p xvision-cli --test exit_codes_eval`
Expected: 6/6 pass.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/tests/exit_codes_eval.rs
git commit -m "test(cli): exit-code coverage for xvn eval (Plan 2b-followup Task 8)"
```

---

## Phase E — Docs + smoke

### Task 9: Add "Exit codes" section to MANUAL + xref READMEs

**Files:**
- Modify: `MANUAL.md`
- Modify: `crates/xvision-skills/README.md`
- Modify: `crates/xvision-engine/README.md`

- [ ] **Step 1: Append exit-code reference to MANUAL.md**

In `MANUAL.md`, after the "AI agent drives xvn" section (around line 327), add:

````markdown
### Exit codes (Plan 2b-followup)

`xvn skill *`, `xvn strategy *`, and `xvn eval *` follow Printing-Press-style
typed exit codes so AI agents can dispatch on the *number*, not the error
text:

| Code | Meaning | Agent should |
|------|---------|--------------|
| 0 | Success | continue |
| 2 | Usage / malformed input / unknown enum variant | re-read `--help`, fix the invocation |
| 3 | Auth (missing or invalid credential) | prompt operator for `ANTHROPIC_API_KEY` or `--mock` |
| 4 | Resource not found (strategy id, skill name, run id) | re-fetch with `xvn <verb> ls`; the id is stale |
| 5 | Upstream / network / disk / database error | retry with backoff |
| 7 | State conflict (e.g. attaching skill to empty slot) | inspect the resource and reconcile state |

Other verbs (`fire-trade`, `venue`, `ab-compare`, `dashboard`, `eod`, …)
default to exit 5 on any error pending per-command opt-in.

```bash
xvn strategy show 01BAD; echo $?      # 4
xvn skill new --from-file /no/such; echo $?    # 2
xvn eval show 01BAD; echo $?          # 4
xvn skill attach <id> --slot regime --skill x; echo $?   # 7 (slot empty)
```
````

- [ ] **Step 2: Add exit-code blurb to `crates/xvision-skills/README.md`**

In the `xvision-skills/README.md`, near the bottom (after the MCP section), add:

```markdown
## Exit codes

`xvn skill {new | ls | attach}` returns Printing-Press-style typed exit codes
(0 / 2 / 3 / 4 / 5 / 7). See the **Exit codes** section in `MANUAL.md`
for the full contract.
```

- [ ] **Step 3: Add same blurb to `crates/xvision-engine/README.md`**

In the engine README, near the CLI quick-start, add:

```markdown
> **Exit codes:** `xvn strategy *` and `xvn eval *` return typed exit codes
> (0 / 2 / 3 / 4 / 5 / 7) — see **Exit codes** in `MANUAL.md`.
```

- [ ] **Step 4: Commit**

```bash
git add MANUAL.md crates/xvision-skills/README.md crates/xvision-engine/README.md
git commit -m "docs: typed exit codes — MANUAL + crate README crossrefs (Plan 2b-followup Task 9)"
```

---

### Task 10: End-to-end smoke + final workspace check

- [ ] **Step 1: Smoke each exit code on the release binary**

```bash
cargo build --release -p xvision-cli
XVN=./target/release/xvn
XVN_HOME=/tmp/xvn-exit-smoke
rm -rf $XVN_HOME

# 0 — success
$XVN strategy templates >/dev/null; echo "templates: $?"   # expect 0

# 2 — usage (unknown template)
$XVN strategy new --template no-such --name x 2>/dev/null; echo "bad template: $?"  # expect 2

# 4 — not found (unknown strategy)
$XVN strategy show 01ZZZZZZZZZZZZZZZZZZZZZZZZ 2>/dev/null; echo "show missing: $?"  # expect 4

# 7 — conflict (attach to empty slot)
ID=$($XVN strategy new --template mean_reversion --name x)
echo "$XVN_HOME/skills" | xargs mkdir -p
echo '---
name: t
display_name: T
description: x
version: 1.0
allowed_tools: []
model_requirement: anthropic.claude-sonnet-4.6
---
body' > /tmp/t.md
$XVN skill new --from-file /tmp/t.md
$XVN skill attach $ID --slot regime --skill t 2>/dev/null; echo "attach empty: $?"   # expect 7
```

Each line above prints the expected exit code.

- [ ] **Step 2: Workspace test sweep**

```bash
cargo test --workspace 2>&1 | grep -E "test result|FAILED" | tail -10
```
Expected: no FAILED. (Test counts go up by ~25 — Phase A:4, B:7, C:6, D:6 + the wrapper exit::tests:4, minus duplicates.)

- [ ] **Step 3: Clippy on plan-touched crates**

```bash
cargo clippy -p xvision-cli --no-deps -- -D warnings
```
Expected: no warnings on the new exit module / commands. Pre-existing warnings in `provider.rs` / `risk.rs` may still surface; if so, this plan does not address them (out of scope).

- [ ] **Step 4: Commit any cleanup**

```bash
git add -A
git commit -m "chore: Plan 2b-followup final workspace check"
```

---

## Self-review checklist

**Spec coverage (against the Printing Press review's typed-exit-codes recommendation):**
- [x] Six exit codes (0/2/3/4/5/7) defined per PP convention.
- [x] `xvn skill *` covered (Tasks 3 + 4).
- [x] `xvn strategy *` covered (Tasks 5 + 6).
- [x] `xvn eval *` covered (Tasks 7 + 8).
- [x] Untyped commands degrade to Upstream automatically (Task 1's `From<anyhow::Error> for CliError`).
- [x] Documented for agents in MANUAL (Task 9).
- [ ] *Out of scope:* per-command rollout to fire-trade / venue / store / dashboard / eod / indicator / intern / trader / risk / provider / ab-compare / run-setup. They keep current behavior; opt-in incrementally.

**Type consistency:** `XvnExit`, `CliError`, `CliResult`, `ResultExt`, `api_to_cli` — used consistently across all 10 tasks. No collisions with existing types.

**No placeholders:** every code block in this plan is real Rust. Tests are spelled out with exact assertions.

**Frequent commits:** 10 tasks → 10 focused commits, one per task.

**Plan-vs-implementation hazards:**
- `mean_reversion` template currently fills only `trader_slot`. Task 4 Step 1 already includes the spot-check command; if that changes, swap the empty-slot test target to `intern`.
- `strategy run` test in Task 6 explicitly `env_remove("ANTHROPIC_API_KEY")` to keep the test deterministic regardless of operator env.
- The `From<anyhow::Error> for CliError` defaulting to `Upstream` is **load-bearing** — without it, every untyped command's `?` operator stops compiling. Task 1 must land before Task 2.

---

## What's next (after this plan ships)

Two adjacent Printing-Press follow-ups identified in `2026-05-11-printing-press-review-xvn-cli.md`:

1. **`--dry-run` on mutations** — small (2-3 commits): add to `xvn skill attach` and `xvn strategy new` (the cheapest wins).
2. **Stdin support for `xvn skill new`** — tiny (1 commit): `--from-file -` reads stdin, lets agents pipe LLM output → `xvn skill new` without a tmpfile.

Neither blocks anything. Skip the rest of the PP audit's recommendations per the explicit "kept our way" decisions in §10 of the review.
