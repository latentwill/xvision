# MCP surface

The Model Context Protocol surface lets an external MCP client (Claude Code,
Cursor, Codex, etc.) invoke a curated subset of `xvn` operations as structured
tool calls without going through the Bash CLI. The server binary is `xvn-mcp`
(crate `xvision-mcp`, installed alongside `xvn`). It speaks MCP over
stdin/stdout (stdio transport) and is stateless for indicator tools — the caller
supplies the price series directly. Authoring and eval tools write to
`$XVN_HOME` just as the CLI does.

MCP is one of two paths an agent can drive xvn (the other is direct CLI
invocation via Bash). For the CLI-first recommended loop, see
[Driving xvn as an agent](/docs?slug=driving-xvn-as-an-agent).

## What is exposed

Two surfaces are registered:

**Indicator tools** — stateless. The caller supplies the full price/HLC series
as parameters; the server dispatches into `xvision-data` and returns a
same-length array. Warmup positions emit as JSON `null`.

**Authoring and eval tools** — stateful. Reads and writes strategy drafts in
`$XVN_HOME/strategies/` and queries `$XVN_HOME/store.db`. Each call opens a
fresh sqlite pool scoped to the request.

| Tool | Wraps | Inputs | Output | Destructive |
|---|---|---|---|---|
| `xvn_health` | health probe | — | `{ok, name, version}` | no |
| `xvn_sma` | `xvision_data::sma` | `prices[]`, `period` | same-length array, nulls for warmup | no |
| `xvn_ema` | `xvision_data::ema` | `prices[]`, `period` | same-length array | no |
| `xvn_rsi` | `xvision_data::rsi` (Wilder) | `prices[]`, `period` | same-length array | no |
| `xvn_bollinger` | `xvision_data::bollinger` | `prices[]`, `period`, `k` | `{middle, upper, lower}` arrays | no |
| `xvn_atr` | `xvision_data::atr` (Wilder) | `high[]`, `low[]`, `close[]`, `period` | same-length array | no |
| `xvn_macd` | `xvision_data::macd` | `prices[]`, `fast`, `slow`, `signal` | `{macd, signal, histogram}` arrays | no |
| `xvn_donchian` | `xvision_data::donchian` | `high[]`, `low[]`, `period` | `{upper, lower}` arrays | no |
| `xvn_fib_retracements` | `xvision_data::fib_retracements` | `prices[]`, `lookback` | `{found, high, low, direction, levels}` | no |
| `xvn_list_templates` | `xvision_engine::authoring::list_templates` | — | `[{name, display_name, plain_summary}]` | no |
| `xvn_create_strategy` | `authoring::create_strategy` | `template`, `name`, `creator?` | `{id}` | yes — writes strategy file |
| `xvn_get_strategy` | `authoring::get_strategy` | `id` | full Strategy JSON | no |
| `xvn_update_slot` | `authoring::update_slot` | `id`, `slot`, `prompt?`, `attested_with?`, `provider?`, `model?`, `allowed_tools?` | `{id, updated}` | yes — mutates strategy |
| `xvn_set_risk_config` | `authoring::set_risk_config` | `id`, `preset` or `explicit` | `{id, applied}` | yes — mutates strategy |
| `xvn_validate_draft` | `authoring::validate_draft` | `id` | `{id, ok, errors}` | no |
| `xvn_strategy_create_atomic` | `api_agents::create` + strategy persist | `name`, `role`, `prompt`, `provider`, `model`, `asset?`, `timeframe?`, `creator?` | `{strategy_id, agent_id, eval_ready, provider, model, warnings}` | yes — writes strategy + agent |
| `xvn_strategy_validate_preflight` | `validate_strategy` + `preflight_validate` | `id`, `scenario_id?` | `PreflightReport` JSON | no |
| `xvn_eval_list` | `api_eval::list_summaries` | `agent_id?`, `scenario_id?`, `status?` | `[RunSummary]` | no |
| `xvn_eval_get` | `api_eval::get_run` | `run_id` | full `RunDetail` | no |
| `xvn_eval_metrics` | `api_eval::get` | `run_id` | `MetricsSummary` or null | no |
| `xvn_eval_scenarios` | `api_eval::scenarios` | — | `[Scenario]` | no |
| `xvn_eval_compare` | `api_eval::compare` | `run_ids[]` (≥2) | `ComparisonReport` | no |
| `xvn_eval_findings` | `RunStore::read_findings` | `run_id` | `[Finding]` | no |
| `xvn_eval_batch_run` | `api_eval::run` × N scenarios | `strategy_id`, `scenario_ids[]`, `mode?`, `review_with?` | `{batch_id, strategy_id, runs[]}` | yes — creates run rows |
| `xvn_eval_batch_status` | `api_eval::get_batch` | `batch_id` | `BatchDetail` | no |
| `xvn_eval_compare_ext` | `api_eval::compare` | `run_ids[]` or `batch_id`, `markdown?` | `ComparisonReport` or Markdown string | no |
| `xvn_scenarios_select` | `api_scenario::list` + filter | `assets[]?`, `timeframe?`, `target_decisions` or `same_decisions+max_decisions`, `regimes[]?`, `count?` | `[{id, name, asset, timeframe, decision_count}]` | no |
| `xvn_eval_compare_report` | `api_eval::compare` + behavior decoration | `run_ids[]` (≥2), `sort?` | `CompareReport` with behavior fields | no |
| `xvn_scenario_inspect_card` | `api_scenario::get` + run aggregate | `id` | `{card: string}` | no |
| `xvn_eval_behavior` | `api_eval::get_run_behavior` | `run_id` | `BehaviorSummary` | no |

## What is deliberately excluded

The same footgun bar that applies to the CLI (see
[Why some commands aren't in xvn](/docs?slug=cli-non-surfaced)) applies here.
The following are not exposed as MCP tools and must not be added without an
explicit operator decision:

- **On-chain side effects** — no `mint`, no `post_reputation`, no
  `post_validation`. Every `xvision-identity` call costs gas, is irreversible,
  and requires the deployer private key.
- **Real-money order placement** — no live order submission against mainnet
  venues. Paper and backtest eval are fine; routing actual orders through an MCP
  tool is a footgun.
- **Filesystem writes outside `$XVN_HOME`** — authoring tools write strategy
  files to `$XVN_HOME/strategies/`; nothing writes outside that root.
- **Direct store mutations** (`insert_decision`, `insert_briefing`,
  `insert_trace`) — these break the replay and metrics reproducibility
  guarantee. Write paths happen only inside the run harness.
- **Live-money trader paths** — `xvn fire-trade --venue orderly` against
  mainnet is not a tool here for the same reason it is excluded from the default
  `xvn` binary surface.

Eval mutation tools (`xvn_eval_batch_run`) invoke backtest mode only inside the
MCP server. Paper mode is accepted as a parameter but does not construct a live
broker; no real orders can be placed via any MCP path.

## Authorization

The MCP surface must be explicitly authorized by the operator before any agent
uses it. Do not enable `xvn-mcp` in a context where an agent runs unattended
without an operator sign-off on the full tool list. The `tools/list` response
enumerates every tool name and input schema; an operator should review it before
granting access.

This mirrors the rule in
[Driving xvn as an agent](/docs?slug=driving-xvn-as-an-agent): "Do not use the
MCP surface without explicit operator authorization."

## How to invoke

`xvn-mcp` is a separate binary installed alongside `xvn`. It is not a
subcommand of `xvn` — it is started as a standalone process by the MCP host.

### Registration in an MCP host

Add the server to your MCP host's `mcpServers` list (e.g. the `xvision-agentd`
Cline sidecar, or Claude Code's `claude_desktop_config.json` — see below):

```json
{
  "mcpServers": [
    {
      "name": "xvision",
      "command": "xvn-mcp",
      "args": [],
      "env": {
        "XVN_HOME": "/path/to/.xvn"
      }
    }
  ]
}
```

`XVN_HOME` must point to the same data root `xvn` uses. If omitted, it falls
back to `$HOME/.xvn`.

### Registration in Claude Code (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "xvision": {
      "command": "xvn-mcp",
      "args": [],
      "env": {
        "XVN_HOME": "/path/to/.xvn"
      }
    }
  }
}
```

### Transport

The server always runs on stdio. Logs go to stderr; stdout is the JSON-RPC
channel. Set `RUST_LOG=info` or `RUST_LOG=xvision_mcp=debug` to see
diagnostic output from the server without corrupting the stream.

### Health check

After registering, verify the wiring with the `xvn_health` tool before issuing
real calls. It returns `{"ok": true, "name": "xvision-mcp", "version": "..."}`.

## See also

- [Driving xvn as an agent](/docs?slug=driving-xvn-as-an-agent) — CLI-first
  recommended agent loop; MCP is the alternative path.
- [Why some commands aren't in xvn](/docs?slug=cli-non-surfaced) — footgun
  inventory and reasoning for every exclusion.
- [CLI Reference](/docs?slug=cli-reference) — full flag reference for equivalent
  `xvn` CLI verbs.
