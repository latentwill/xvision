# Agents

An `Agent` is a reusable composition of `{ prompt, model, provider, skills,
temperature, max_tokens, inputs_policy, prompt_version }` stored in the
workspace agent library. Agents are authored once and referenced by id from
any number of strategies — the same `agent_id` can appear in multiple
strategies simultaneously. Editing an agent propagates to every strategy that
references it on its next run.

This library-centric design is the wave-A "agent library" concept. The wave-C
atomic-create flow separated agent authoring from strategy creation so agents
are first-class reusable objects, not inline strategy slots.

> **Rename in progress** — the `intern` role is being renamed to "default
> agent" in the wizard, rail, and settings UI. Both terms remain valid during
> the crossover period. The underlying field is a free-form string so
> strategies written with either name continue to work.

## AgentSlot anatomy

Each `Agent` contains one or more `AgentSlot`s. A single-slot agent is the
80% case; multi-slot agents model sequential or graph-shaped pipelines within
one agent (distinct from multi-agent pipelines within a strategy).

| Field | Type | Notes |
|---|---|---|
| `name` | `String` | Slot label within this agent, e.g. `"main"`, `"analyst"`, `"executor"`. |
| `provider` | `String` | Provider id, e.g. `"openai"`, `"anthropic"`. Must match a configured provider. |
| `model` | `String` | Model id forwarded to the provider, e.g. `"gpt-4o-mini"`, `"claude-sonnet-4-6"`. |
| `system_prompt` | `String` | The system prompt template handed to the LLM at dispatch time. |
| `skill_ids` | `Vec<String>` | ULIDs into the workspace skill registry (tool / prompt_fragment / evaluator). Picker is hidden in v1 until `/agents/skills` ships; field is persisted now so existing agents survive the registry landing without a migration. |
| `max_tokens` | `Option<u32>` | Optional per-request token budget override. `null` means "auto from model"; the dispatcher resolves it via model metadata. `Some(n)` is honored and passed through verbatim. |
| `temperature` | `Option<f64>` | Optional sampling temperature override. `null` lets the provider default apply. Set a low value (e.g. `0.2`) for reproducible eval baselines. |
| `prompt_version` | `String` | Server-computed 16-hex-char SHA-256 prefix of `system_prompt`. Backfilled on next save for rows persisted before migration 019. Read-only — any value sent on write is overridden at persist time. |
| `inputs_policy` | `InputsPolicy` | Controls how the eval executor sanitizes seed JSON before the LLM sees it. One of `"raw"` (default), `"causal"`, or `"oracle"`. See below. |

### inputs_policy values

- **`raw`** — default. Seed JSON is passed through unchanged; `decision_index`
  lives on the top-level seed and each `bar_history` entry carries `timestamp`.
- **`causal`** — `decision_index` is stripped from the top-level seed; each
  `bar_history` entry's `timestamp` is replaced with `bar_index` (0 = oldest
  visible bar). Matches the v4 causal prompts.
- **`oracle`** — tag-only; runtime behavior is identical to `raw`. Use to
  explicitly mark a slot as deliberately full-visibility rather than
  "left at default."

## Roles

By convention, slot names and strategy roles follow these labels:

| Role | Description |
|---|---|
| `intern` / default agent | General-purpose decision-making slot. `intern` is the legacy name; the UI is migrating to "default agent". |
| `trader` | Proposes a trade decision. |
| `risk_check` / `risk` | Vetoes or modifies the trader's proposal. |
| `executor` | Commits the final decision after risk review. |
| `analyst` | Produces a structured thesis consumed by a downstream slot. |

Role is a plain string label on `AgentRef`, not an enforced enum. Strategies
can freely rename or invent roles — the engine matches role strings to pipeline
stages via case-insensitive trimmed comparison.

## Authoring agents

Agents are created through the dashboard at `/agents/new` or via the chat
rail. There are three starter templates in the template picker:

- **Single-prompt trader** (`single-trader`) — one slot, one model, one
  prompt. Start here for the 80% case.
- **Analyst → Executor** (`analyst-executor`) — two slots demonstrating
  sequential composition. First slot produces a thesis; second turns it into
  a decision.
- **Risk-checked trader** (`risk-checked-trader`) — three slots showing
  trader / risk_check / executor composition.

Template slot names seed the form only. They are not enforced — rename or
extend freely after creation.

The CLI does not have a `create` verb today. That is intentional: authoring
requires the form UI for slot composition and template selection. The CLI
exposes a read-only path for automation scripts.

## Reading agents from the CLI

`xvn agent get` fetches a single agent by ULID. The JSON output is
structurally identical to the `agents[]` array inside `EvalRunExport` — both
use the same `Agent` Rust struct and the same `Serialize` impl, so scripts
that consume eval exports can use the same parsing logic for direct agent
lookups.

```
xvn agent get <agent-id>
xvn agent get <agent-id> --format json-compact
```

`--format` accepts `json` (default, pretty-printed) or `json-compact`
(single-line, suitable for piping to `jq`). `get` also accepts a `show`
alias.

Example output shape:

```json
{
  "agent_id": "01HZAGENT000000000000001",
  "name": "momentum-trader-v2",
  "description": "GPT-4o momentum strategy with causal inputs",
  "tags": ["momentum", "causal"],
  "archived": false,
  "created_at": "2026-05-01T09:00:00Z",
  "updated_at": "2026-05-15T14:22:00Z",
  "slots": [
    {
      "name": "main",
      "provider": "openai",
      "model": "gpt-4o-mini",
      "system_prompt": "You are a discretionary trader...",
      "skill_ids": [],
      "max_tokens": 2048,
      "temperature": 0.2,
      "prompt_version": "a3f9c1e247b80d6f",
      "inputs_policy": "causal"
    }
  ]
}
```

The `max_tokens: null` sentinel in storage serializes as `null` in JSON (not
`0`); `Some(2048)` serializes as the integer `2048`.

## How strategies reference agents

A strategy references agents via `AgentRef { agent_id, role }`:

```json
{
  "agents": [
    { "agent_id": "01HZAGENT000000000000001", "role": "trader" },
    { "agent_id": "01HZAGENT000000000000002", "role": "risk_check" }
  ]
}
```

The same `agent_id` can appear in multiple strategies. Role lives on the
reference, not on the agent — the agent carries no knowledge of the roles
assigned to it by strategies.

## Migrating legacy inline-slot strategies

Strategies created before wave-A carried fixed `intern_slot`, `trader_slot`,
and `regime_slot` fields instead of `AgentRef` pointers. The migration
command promotes those inline slots to first-class agent records and rewrites
the strategy to reference them:

```
xvn strategy migrate-agents
xvn strategy migrate-agents --dry-run
```

`--dry-run` prints what would change without writing to disk. Run without
`--dry-run` to apply. After migration each previously-inline slot becomes a
named agent in the library, reusable by any strategy.

## Provider and model resolution

Each `AgentSlot` binds a provider id and model id. Resolution can drift
silently when a provider is disabled or a model id is removed from the
provider's model list. Run `xvn strategy validate <id>` to surface these
warnings before a run.

See [Providers](/docs?slug=providers) for configuring and enabling providers.

## Archiving agents

Agents can be archived from the dashboard agent detail page. Archived agents
are hidden from the library list by default but are not deleted; strategies
that reference an archived agent continue to resolve it. Pass
`include_archived: true` on the list request to show archived agents.
