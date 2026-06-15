# CLI surface — what `xvn` does NOT expose, and why

> Companion to `MANUAL.md`. The `xvn` binary is the agent-facing control plane;
> this file enumerates the deliberate gaps. Each item is either operator-only,
> footgun-shaped, or already covered by another binary.

The bar for surfacing in `xvn`: an autonomous agent should be able to drive any
exposed command without burning real money, posting on-chain garbage, or
silently corrupting the eval substrate.

---

## Not surfaced — operator-only / on-chain side effects

### `xvision-identity` (mint, post_reputation, post_validation)

**Reason:** every call costs MNT and is irreversible.

- `IdentityClient::register` mints an ERC-8004 NFT against the deployer wallet.
- `IdentityClient::post_reputation` writes to the on-chain Reputation Registry.
- A future Validation receipt path (SLF5) writes signed-oracle backtest receipts.

Operator memory: **xvision is non-custodial for trading capital — the smart
contract surface only routes marketplace fees**. Adding identity calls into the
ergonomic agent CLI collapses identity into the same surface as paper-trading
commands and pushes the deployer key onto the agent's hot path.

Workspace also excludes `xvision-identity` from `default-members` to keep the
heavy `alloy v2` stack out of the dev loop. Promoting identity into `xvn` would
force every `xvn` build to compile alloy.

**Where it lives instead:** `MANUAL.md` §M7 (operator-side runbook), and a
bespoke driver `examples/mint_identity.rs` invoked through
`cargo run --release -p xvision-identity --example mint_identity`.

**When this can change:** add a separate `xvn-identity` opt-in binary (its own
crate target, behind a `--features identity` gate) when SLF3 lands. Never mix
on-chain writes into the default `xvn` binary.

### Live order submission via `fire-trade --venue orderly` against mainnet

`xvn fire-trade --venue orderly` is wired, but defaults read `ORDERLY_BASE_URL`
from env. Operators are expected to set this to the testnet URL
(`https://testnet-api-evm.orderly.org`) until F5 onboarding clears. There is no
guard rail in the CLI itself — keep mainnet credentials out of the shell unless
forward paper has cleared.

---

## Not surfaced — would corrupt the eval substrate

### Arbitrary `Store` writes (insert_decision / insert_briefing / insert_trace)

**Reason:** the SQLite flight-recorder is the source of truth for replay and
metrics. Letting an agent invent rows breaks Tier 1 fix #1 (paired briefings)
and the reproducibility story.

Read paths (`xvn show-decision`, `xvn show-briefing`, `xvn store stats`) are
fine. Write paths happen only inside the harness, never as a CLI primitive.

### Direct risk config edits

**Reason:** risk thresholds are committed in `config/risk.toml` and
`config/whitelist.toml`. Editing them per-run via CLI defeats the audit trail.

Mutation goes through git on those TOML files. (The `xvn risk` command was
retired in 2026-06 when `xvision-risk` was deleted.)

---

## Not surfaced — already covered elsewhere

### `xvn-mcp` (deprecated)

The MCP indicator server is its own binary (`crates/xvision-mcp/src/main.rs`,
installed as `xvn-mcp`). Its original purpose was to be advertised by ACPX —
which has been removed (2026-05-10). Indicator computation now lives directly
on the CLI as `xvn indicator <name>`, which is the preferred path for agents
driving xvn through Bash. The crate is left in the workspace for any external
MCP client (Claude Code's MCP support, etc.) that wants to register it
manually, but it is no longer part of the recommended agent surface.

### `xvn-anything-strategy-internal`

Strategy authoring (`xvn strategy *`) covers create / validate / ls / show /
templates / run. Tools (`OhlcvTool`, `IndicatorPanelTool`) are reachable
through the pipeline; surfacing them as standalone CLI subcommands duplicates
what `xvn strategy run` already exercises.

---

### Stage 1 Intern subprocess and `xvn intern` (removed 2026-06)

Previously, the two-stage Intern→Trader architecture included a separate Stage 1
(briefing) run via `xvn intern brief/preview`. This stage has been retired and
folded into the single-stage agent model (2026-06). Agents now dispatch directly
without a separate briefing stage. If multi-step tool-use ever lands again it
will arrive as a new backend impl, not as a subprocess shim.

## Surfaced as of this audit

See `crates/xvision-cli/src/lib.rs` for the live list. The current set, in
addition to the pre-existing commands, exposes:

- `xvn trader run` / `xvn trader preview` — Agent dispatch in isolation
- `xvn portfolio --venue {alpaca,orderly}` — read live venue state
- `xvn close-position --venue {alpaca,orderly} --asset BTC` — flatten one symbol
- `xvn fire-trade --venue {alpaca,orderly}` — extends the Alpaca-only path
- `xvn show-briefing --setup-id <uuid>` — read cached Intern briefing
- `xvn store migrate` / `xvn store stats` — explicit DB ops
- `xvn metrics` / `xvn gate` — pre-committed metrics + anti-overfit verdict
- `xvn indicator <name>` — compute one indicator from a JSON price series
- `xvn provider {list,show,check,add,remove}` — manage the LLM provider
  registry in `config/default.toml`. `add` / `remove` mutate the file in place via
  `toml_edit` (comments preserved); `check` is a TCP-connect smoke with an
  opt-in `--probe` that GETs `<base_url>/models`. See migration note
  `docs/migrations/2026-05-10-providers-config.md`.

Everything else inside the workspace (engine internals, eval bootstrap helpers,
backtest sim executor, `tracing-subscriber` config) is library-only. If a real
agent workflow needs one of those, add the CLI shim in this same pattern: a
thin wrapper in `crates/xvision-cli/src/commands/`, no business logic.

*Last updated: 2026-05-10.*
