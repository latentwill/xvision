# Agents

An agent is a reusable saved bundle of a system prompt, a provider/model
selection, a temperature, an output token budget, and an optional set of
skills. Agents live in the workspace library and are referenced by id from
strategies — the same agent can appear in multiple strategies simultaneously.
Editing an agent propagates to every strategy that references it on its next
run.

> **Capability set update** — the Capability enum is now exactly { Trader,
> Filter, Router }. The Intern and Critic stages have been retired and
> folded into the single-stage agent model.

---

## Author an agent from the dashboard

Open `/agents/new`. The form has three sections:

- **Identity** — name (slug-style, e.g. `btc-mean-rev-v1`), description,
  and optional tags.
- **Behavior (slots)** — one or more agent slots. Each slot exposes:
  - **Provider** — picked from your enabled providers.
  - **Model** — picked from the models available for that provider.
  - **System prompt** — the prompt handed to the model at dispatch time.
  - **Skills** — attached skills from the workspace skill registry (managed
    at `/agents/skills`; shown only when skills are already linked).
- **Template picker** — before reaching the form, the dashboard offers three
  starter templates: *Single-prompt trader* (one slot), *Analyst → Executor*
  (two slots, sequential), and *Risk-checked trader* (three slots,
  trader / risk / executor). Templates seed the form; rename or extend freely
  after creation. You can also skip to a blank agent.

**Chat rail path:** describe the agent you want in plain English and the rail
composes one for you, then routes to the same form pre-filled.

The CLI does not have a create verb today — authoring uses the dashboard form.

---

## Memory

Every agent slot has a **Memory** selector, right next to the provider
and model pickers. It controls whether the slot carries anything
forward between cycles and between runs. It is **off by default** and
stays off until you change it — a slot with the selector untouched
behaves exactly like agents did before memory shipped.

Three positions:

- **Off** — no memory. Nothing is recorded, nothing is recalled. Use
  this when you want a clean A/B between two prompts or a backtest
  with hard guarantees that the agent only sees what's in its prompt.
- **Global** — the slot shares a memory pool with every other slot
  also set to Global. Useful when several agents in a strategy should
  pool what they learn.
- **Agent-scoped** — the slot has its own private memory keyed to the
  agent id. Other agents, including other slots in the same strategy,
  can't see it.

In v1, Memory mostly records — the recall side ships empty by design,
so you'll see the Memory panel on the eval-review page sit at "no
recall items" even with Global or Agent-scoped selected. That's
expected; the design behind it (and the reason backtest replays
don't leak future knowledge) is covered in the full memory
overview at `docs/v2d-memory-overview.md` in the repo.

### Managing memory in the dashboard

Two surfaces exist for browsing and seeding memory:

- **Memory tab on this page.** Sits between **Configuration** and the
  tail of the agent detail. Patterns + Observations sub-tabs scoped
  to this agent's namespace.
  - Patterns sub-tab — list, add (`+ Add Pattern` modal), delete one
    by one, or **Forget all** at the bottom.
  - Observations sub-tab — read-only, filterable by scenario and
    run. Bulk-forget only; no per-row delete.
- **Workspace `/memory` page.** Sidebar entry between `/scenarios`
  and `/eval-runs`. Same Patterns + Observations split, scoped to the
  shared `global` namespace — the pool every Global-mode slot reads
  from.

The MemoryPanel on the eval-review page has an overflow menu (`⋯`)
on each recall row with **Open Pattern** — clicks deep-link into
whichever of the two pages owns the Pattern, with the row highlighted
and scrolled into view. The resulting URL is shareable.

### Scripted alternative — `xvn memory add-pattern`

For automation and one-shot seeding:

```
xvn memory add-pattern "BTC ranges before FOMC" --namespace global --training-end 2024-09-01
```

See the full memory overview at `docs/v2d-memory-overview.md` for the
other `xvn memory` verbs (`ls`, `show`, `rm`, `forget`) and the
`training_window_end` semantics.

---

## Read an agent from the CLI

```
xvn agent get <agent-id>
xvn agent get <agent-id> --format json-compact
```

`--format` accepts `json` (default, pretty-printed) or `json-compact`
(single-line, suitable for piping to `jq`). `show` is an accepted alias for
`get`.

Example output:

```json
{
  "agent_id": "01HZAGENT000000000000001",
  "name": "momentum-trader-v2",
  "description": "GPT-4o momentum strategy",
  "tags": ["momentum"],
  "archived": false,
  "created_at": "2026-05-01T09:00:00Z",
  "updated_at": "2026-05-15T14:22:00Z",
  "slots": [
    {
      "name": "main",
      "provider": "openai",
      "model": "gpt-4o-mini",
      "temperature": 0.2
    }
  ]
}
```

---

## Roles

By dashboard convention, agents play named roles in a strategy. Roles are
display labels set on the strategy's agent references via the Inspector — they
live on the reference, not on the agent itself.

| Role | Description |
|---|---|
| default agent / `intern` | General-purpose decision slot. "intern" is the legacy name; the UI is migrating to "default agent". |
| `trader` | Proposes a trade decision. |
| `risk` / `risk_check` | Vetoes or modifies the trader's proposal. |
| `executor` | Commits the final decision after risk review. |
| `analyst` | Produces a structured thesis consumed by a downstream slot. |

Roles are plain strings — strategies can freely rename or invent them.

---

## Capabilities

A role is a free-text label on an `AgentRef`; a **capability** is the typed
contract a slot fulfils. Capabilities are what the engine reasons about for
launch readiness and for what can be optimized. The recognized set:

| Capability | Meaning | Required tools | Optimizable |
|---|---|---|---|
| `trader` | Proposes a sized trade decision. | `ohlcv` | yes (DSPy signature) |
| `filter` | Deterministic / model gate over candidate cycles. | (per filter) | yes (DSPy signature) |
| `router` | Routes decisions to downstream agents. | — | no |
| `decision_grader` | Scores a decision against an outcome (used as an optimizer metric). | — | no |
| `chat_authoring` | Composes strategies/agents from the chat rail. | — | no |

Only `trader` and `filter` have DSPy signatures today, so only those can be
fed to `xvn optimize`. The rest are reported by diagnostics but cannot be
tuned yet.

---

## Diagnostics & launch readiness

Before a strategy can launch, every **required** capability needs a slot with a
prompt, a model binding, its required tools, and a runtime that supports it.
Two CLI surfaces report this:

```
xvn agent inspect <agent-id> --diagnostics          # per-agent, strategy-independent
xvn strategy diagnostics <strategy-id>              # whole strategy, launch-gated
```

`xvn agent inspect --diagnostics` reports each capability's `has_prompt`,
`has_model_binding`, `required_tools`, `runtime_supported`, and `optimizable`
flags — it describes state and always exits `0` for a resolved agent (an
incomplete agent is not an error on its own).

`xvn strategy diagnostics` is the launch gate: it rolls the per-agent state up
across the whole strategy and exits `14` (`OptValidation`) when any required
capability is unmet, listing each gap with a typed reason
(`missing_tool` / `missing_prompt` / `missing_model_binding` / `unsupported`).
The same typed statuses surface in the dashboard as readiness badges and in the
unified chat-rail event stream as `error_missing_capability` /
`error_missing_tool` rows — nothing short-circuits silently. Full flag and JSON
shapes are in [CLI Reference → Capability diagnostics](/docs?slug=cli-reference).

In the dashboard, the agent edit page shows the same readiness state inline; a
not-launchable strategy renders the unmet list as remediation rather than
blocking with a popup.

---

## Improve this agent (tune → candidate → accept → swap)

An agent slot whose capability is optimizable (`trader` / `filter`) can be
**tuned offline** and the winner accepted as a new child agent. The flow:

1. **Tune.** Run an offline optimization pass over a corpus for the slot's
   capability:

   ```
   xvn optimize run \
     --agent <agent-id> --slot <slot> --capability trader \
     --corpus ./corpus.json --optimizer mipro --metric delta_sharpe \
     --rng-seed 42 --json
   ```

   This produces candidate instructions scored by the metric, persists them to
   the optimization store, and records a winning **snapshot**. It runs against a
   deterministic no-network model by default; the engine pulls in no DSPy
   dependency (see [Optimizer](/docs?slug=optimizer)).

2. **Inspect the candidate.** Review the candidate table and the prompt diff:

   ```
   xvn optimize inspect <run-id> --json
   ```

   In the dashboard, open the optimization run detail (linked from the
   **Improve this agent** panel on the agent edit page) to see the candidate
   table, the prompt diff against the parent, the metric delta, and the
   train/holdout split inline.

3. **Accept as a child agent.** Accepting clones the parent agent, swaps the
   optimized slot's system prompt for the winning candidate's instruction, and
   records a lineage edge `parent → child`:

   ```
   xvn optimize accept-as-child-agent <snapshot-id>
   ```

   The parent is left unchanged. Accept is reversible:

   ```
   xvn optimize revert-accepted <snapshot-id>
   ```

   clears the accept flag and the lineage edge.

4. **Swap into a strategy.** The child agent is a normal library agent — wire it
   into (or swap it within) a strategy with the usual reference verbs:

   ```
   xvn strategy add-agent <strategy-id> <child-agent-id> --role trader
   xvn strategy remove-agent <strategy-id> --role trader
   ```

> Acceptance is **holdout-disciplined**: a snapshot whose winner was selected on
> training data only, with no holdout split, is refused at accept time. You tune
> on train, you confirm on holdout, then you accept. See
> [Optimizer → Holdout discipline](/docs?slug=optimizer).

---

## Lineage

Every accepted child records an `agent_lineage` row tying the child agent back
to its parent and to the optimization run that produced it:

```json
{
  "snapshot_id": "01KSEJ198AN5QK9QTA1ZFXSXST",
  "child_agent_id": "01CHILDAGENT",
  "parent_agent_id": "01PARENTAGENT",
  "optimization_run_id": "01KSEJ1989S8N1DHVBVR9KWW9M",
  "accepted": true
}
```

The optimization run itself persists a reproduction recipe (corpus query, RNG
seed, model, optimizer + version, signature hash, metric) so any accepted child
can be traced to — and re-derived from — the exact inputs that produced it.
`revert-accepted` clears both the accept flag and the lineage edge, so an
accepted child can be unwound without losing the run record.

---

## CLI verbs at a glance

See [CLI Reference](/docs?slug=cli-reference) for full flag documentation.

| Verb | Effect |
|---|---|
| `xvn agent get <agent-id> [--format json\|json-compact]` | Fetch a single agent by id. Alias: `show`. |
| `xvn agent inspect <agent-id> --diagnostics [--json]` | Per-capability readiness for one agent (prompt / model / tools / runtime / optimizable). |
| `xvn strategy diagnostics <strategy-id> [--json]` | Whole-strategy launch readiness; exits `14` when not launchable. |
| `xvn optimize run …` | Offline tune of an optimizable slot/capability. |
| `xvn optimize accept-as-child-agent <snapshot-id>` | Mint a tuned child agent + lineage edge. |
| `xvn optimize revert-accepted <snapshot-id>` | Unwind an accepted child. |

---

## Where agents live

Agents are stored in the workspace database at `$XVN_HOME/xvn.db` (defaults to
`$HOME/.xvn/xvn.db`; override with `--xvn-home` or the `XVN_HOME` env var).
The database is created automatically on first use; no separate init step is
required.
