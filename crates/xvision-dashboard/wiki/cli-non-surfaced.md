# Why some commands aren't in `xvn`

`xvn` is the agent-facing control plane. The bar for surfacing a command: an
autonomous agent must be able to drive it without burning real money, posting
irreversible on-chain data, or silently corrupting the eval substrate.

Commands that fail that test are deliberately excluded. Each exclusion falls into
one of three categories below.

## Not surfaced: on-chain side effects

### `xvision-identity` (mint, post_reputation, post_validation)

Every call costs MNT gas and is irreversible. `IdentityClient::register` mints
an ERC-8004 NFT against the deployer wallet; `post_reputation` writes to the
on-chain Reputation Registry. Mixing these into the ergonomic agent CLI would
push the deployer private key onto the agent's hot path.

`xvision-identity` is also excluded from `default-members` in `Cargo.toml` to
keep the `alloy v2` dependency out of the standard dev build. Promoting it into
`xvn` would force every `xvn` build to compile alloy.

**Use instead:** the operator-side mint steps in
[Operator Manual](/docs?slug=operator-manual), and a bespoke
`examples/mint_identity.rs` driver invoked via
`cargo run --release -p xvision-identity --example mint_identity`.

**When this changes:** a separate `xvn-identity` binary (its own crate target,
behind `--features identity`) can be added when on-chain identity ships. On-chain
writes must never be part of the default `xvn` binary.

### Live order submission via `xvn fire-trade --venue orderly` against mainnet

`xvn fire-trade --venue orderly` is wired and functional. It reads
`ORDERLY_BASE_URL` from the environment. There is no CLI guardrail — operators
must keep mainnet credentials out of the shell until forward-paper eval has
cleared.

**Use instead:** set `ORDERLY_BASE_URL=https://testnet-api-evm.orderly.org`
until you are ready for live trading.

## Not surfaced: would corrupt the eval substrate

### Arbitrary `Store` writes (insert_decision, insert_briefing, insert_trace)

The SQLite flight-recorder is the source of truth for replay and metrics.
Allowing an agent to insert synthetic rows breaks decision/briefing pairing and
the reproducibility guarantee.

Read paths are fine: `xvn show-decision`, `xvn show-briefing`, `xvn store stats`.
Write paths happen only inside the run harness, never as CLI primitives.

### Direct risk config edits

Risk thresholds live in `config/risk.toml` and `config/whitelist.toml`. Editing
them per-run via CLI would defeat the audit trail.

Edit the TOML files through git for any change. (The `xvn risk` command was
retired in 2026-06 when `xvision-risk` was deleted.)

## Not surfaced: covered elsewhere

### `xvn-mcp`

The MCP indicator server (`crates/xvision-mcp/src/main.rs`, installed as
`xvn-mcp`) is its own binary. Indicator computation is now also available
directly on the CLI as `xvn indicator <name>`, which is the preferred path for
agents driving xvn through Bash. The `xvn-mcp` binary remains in the workspace
for external MCP clients (e.g. Claude Code's MCP support) that want to register
it manually.

### Strategy internals (`OhlcvTool`, `IndicatorPanelTool`, etc.)

`xvn strategy *` already covers create / validate / ls / show / templates / run.
Internal pipeline tools are reachable through the strategy run surface;
surfacing them as standalone subcommands would duplicate what `xvn strategy run`
already exercises.

## What is surfaced

The current surface, for reference:

| Command group | What it does |
|---|---|
| `xvn strategy *` | Create, validate, list, show, run strategies |
| `xvn eval *` | Launch and inspect eval runs |
| `xvn scenario *` | Create, classify, inspect scenarios |
| `xvn trader run / preview` | Agent dispatch in isolation |
| `xvn portfolio --venue` | Read live venue state (read-only) |
| `xvn close-position --venue --asset` | Flatten one symbol |
| `xvn fire-trade --venue` | Submit a trade (paper or testnet) |
| `xvn show-briefing / show-decision` | Read cached run data |
| `xvn store migrate / stats` | Explicit DB ops |
| `xvn metrics / xvn gate` | Pre-committed metrics + anti-overfit verdict |
| `xvn indicator <name>` | Compute one indicator from a JSON price series |
| `xvn provider *` | Manage LLM provider registry |

Everything else in the workspace is library-only. To add a CLI shim for a new
workflow, add a thin wrapper in `crates/xvision-cli/src/commands/` with no
business logic.

## See also

- [CLI Reference](/docs?slug=cli-reference) — full flag reference for every surfaced `xvn` command.
- [Operator Manual](/docs?slug=operator-manual) — operator-side tasks for unsurfaced operations.
