# Optimizer

The optimizer tunes an agent slot's prompt + demonstrations offline, scores
candidates against a metric on a corpus, and lets you accept the winner as a
**child agent** with a recorded lineage edge. It is a research/authoring tool —
it never runs inside an eval or on the live decision path.

It is driven from the CLI (`xvn optimize …`) or launched from the dashboard's
**Improve this agent** panel, and surfaced read/write in the dashboard. The
objective: search over the things a `Strategy`/`Agent` can vary (instruction
text, demonstrations) as *data*, instead of hand-editing prompts, so a
DSPy-style optimizer can reach hypotheses a hardcoded harness can't.

---

## Offline-only invariant (load-bearing)

**The DSPy stack never enters the engine or the slim runtime image.** This is a
hard architectural rule, not a preference:

- `xvision-dspy` is its own crate, **excluded from `default-members`**. Its
  ~93-package transitive tree (`dspy-rs`, `rig-core`, arrow/parquet, foyer,
  hf-hub) is therefore kept out of `xvision-engine`, `xvision-cli`, and
  `xvision-dashboard` builds, and out of the shipped image.
- The engine's `optimization` store treats snapshots and demos as **opaque
  JSON blobs**. It never parses a `xvision-dspy` type. The dashboard's
  optimizations routes read that store and surface candidate `instruction`
  strings and the opaque `snapshot_json` as-is.
- Proof, run on the merged branch:

  ```
  $ cargo tree -p xvision-engine | grep -iE "dspy-rs|xvision-dspy" || echo "engine clean"
  engine clean
  $ cargo tree -p xvision-dashboard | grep -i dspy || echo clean
  clean
  ```

`accept-as-child-agent` is what bridges the two worlds **without** a dependency:
it clones the parent agent and swaps the optimized slot's `system_prompt` for
the selected candidate's plain instruction string. A plain string crosses the
boundary; no DSPy type does.

> If you are contributing code: anything that would make `xvision-engine` or
> `xvision-dashboard` pull `dspy-rs`/`rig-core` is a regression. Keep optimizer
> logic in `xvision-dspy`; persist results as JSON the engine can store blindly.

---

## DummyLM in CI (deterministic, no network)

CI exercises the optimizer with `dspy-rs`'s built-in `DummyLM` — a deterministic
language-model double. Every optimizer test (compile, run, each failure class,
the success/inspect round-trip) runs with no network and no provider key, and a
fixed `--rng-seed` makes the run reproducible: the same seed + inputs yields the
same winning candidate (`same_seed_yields_same_winner`).

`xvn optimize run` resolves the agent's bound `provider`/`model` from the agent
store and records it in the optimization run for provenance. Pass `--test-model`
to skip resolution and use a `dummy/dummy` identity instead (CI / offline use).

```
# default: uses the agent's bound provider+model
xvn optimize run --agent … --slot … --capability trader \
  --corpus ./corpus.json --optimizer mipro --metric delta_sharpe --rng-seed 42 --json

# CI / offline: skip agent lookup, use dummy/dummy
xvn optimize run --agent … --slot … --capability trader \
  --corpus ./corpus.json --optimizer mipro --metric delta_sharpe --rng-seed 42 --test-model --json
```

---

## Optimizers: MIPRO / GEPA / COPRO

`--optimizer` selects the search algorithm. All three search the same space
(instruction + demonstrations) but differ in strategy:

| Optimizer | Shape | Notes |
|---|---|---|
| `mipro` | Multi-prompt instruction + demo proposal/search. | Default workhorse. |
| `gepa` | Reflective / evolutionary prompt search. | |
| `copro` | Coordinate-ascent instruction refinement. | Cheapest; refines one instruction line per round. |

`--max-rounds` (default `4`) bounds the search. The optimizer-internal details
(MIPRO/GEPA proposal mechanics) are hidden in the dashboard until an **Advanced**
detail toggle is opened — the default surface is the candidate table, the prompt
diff, and the metric delta.

`--capability` must have a DSPy **signature** to be optimizable. Today that is
`trader` and `filter`; requesting an unsupported capability (`router`,
`decision_grader`, `chat_authoring`) fails with exit `11`
(`OptMissingCapability`, the typed `missing_capability_optimizer` error).

---

## Snapshots, demos, and lineage

A run persists (migration `045`) five related kinds of row:

- **`optimization_runs`** — one row per `xvn optimize run`: agent/slot/capability,
  optimizer + version, metric, corpus query, `rng_seed`, `signature_hash`,
  model provider/name, and `status`.
- **`optimization_candidates`** — the proposed instructions, each with its
  `metric_value`, its `split` (`train` / `holdout`), its `demo_set`, and a
  `selected` flag on the winner. Ordered by candidate index.
- **`demos`** — content-addressed (deduplicated) demonstration sets. The same
  demo set referenced by multiple candidates/snapshots is stored once.
- **`optimization_snapshots`** — the accept-able artifact: the winning
  instruction + demo set + the reproduction recipe. Carries the accept flag.
- **`agent_lineage`** — the `parent → child` edge written on
  `accept-as-child-agent`, tying the minted child agent to the snapshot and the
  run that produced it.

Every run is **reproducible from its persisted inputs**. The reproduction recipe
captures the corpus query, RNG seed, model provider/name, optimizer + version,
signature hash, and metric — enough to re-derive the same result:

```json
{
  "corpus_query": "…/corpus.json",
  "rng_seed": 7,
  "model_provider": "dummy",
  "model_name": "dummy",
  "optimizer": "copro",
  "optimizer_version": "dspy-rs-0.21.0",
  "signature_hash": "75e530003c74ddac820af911d37fc4e1fff3e3de5e415fe79fdcb5b763027afe",
  "metric": "delta_sharpe"
}
```

`POST /api/optimizations/:id/accept` records the lineage edge and leaves the
parent unchanged; `POST /api/optimizations/:id/revert` clears the accept flag
and the edge. Both are dashboard API endpoints; there is no longer a separate
CLI verb — the dashboard's **Improve this agent** panel triggers the full
cycle. A `FAILED` run still keeps its partial candidates so the evidence isn't
lost.

Export keeps demos portable across workspaces via the cycle-level export:

```
xvn optimize export <run-id> --output snapshot.json
```

---

## Holdout discipline

Tuning on data you also score on overfits. The optimizer enforces a
train/holdout split:

- Candidates are scored on a `train` split during search and confirmed on a
  `holdout` split before a winner is trusted. Each candidate's `split` is
  recorded.
- **Accept is refused without a holdout.** A snapshot whose winner was selected
  on training data only — no holdout confirmation — cannot be accepted as a
  child agent. You tune on train, confirm on holdout, then accept.
- The dashboard run-detail renders the train/holdout split alongside the metric
This is the optimizer-side analogue of the anti-overfit holdout discipline on
eval metrics: a measured improvement only counts if it survives data the search
never saw. The `holdout_min_improvement` setting in `autooptimizer.toml`
enforces the minimum delta required for acceptance.

## Gate dimensions

The optimizer gate evaluates each candidate against the parent across five
dimensions. All five must pass for a candidate to be accepted:

| # | Dimension | Config key | What it guards against |
|---|---|---|---|
| 1 | **Min-trade retention** | `min_trade_retention_ratio` | 0-trade degenerate strategies that game Sharpe by refusing to enter. Child must retain at least the configured fraction of the parent's fill legs, with a hard floor of 1. Default 0.5 (50%). |
| 2 | **Delta score (day)** | `min_improvement` | In-sample improvement must exceed threshold. Default 0.05 (5%). |
| 3 | **Delta score (holdout)** | `holdout_min_improvement` | Out-of-sample improvement must exceed threshold. Default 0.005 (0.5%). |
| 4 | **Drawdown guard** | _(hardcoded)_ | Child worst drawdown must not exceed 1.5× parent worst drawdown. |
| 5 | **Realized-return ratio** | `min_realized_return_ratio` | "Open and hope" strategies with strong mark-to-market but negligible booked profit. At least the configured fraction of total return must come from closed positions. Skipped when total return ≤ 0. Default 0.25 (25%); set to 0.0 to disable. |

The trade-retention and realized-return checks are non-objective risk guards —
they run regardless of which optimization objective (`sharpe`, `total_return`,
`max_drawdown`, `win_rate`) is selected. All five checks run to completion so
the rejection reason surfaces every failing dimension.
---

## Surfaces

- **CLI** — `xvn optimize run` (launch), `xvn optimize inspect` (read results), `xvn optimize diff` (compare candidates), `xvn optimize export` (portable snapshot), `xvn optimize lineage ls/show` (trace child agent ancestry). Distinct exit codes 10–15 per failure class; see [CLI Reference](/docs?slug=cli-reference).
  - `GET /api/optimizations?agent=&slot=` — list runs, slot filter narrows.
  - `GET /api/optimizations/:id` — run detail: candidate table, snapshot, lineage.
  - `POST /api/optimizations/:id/accept` — mint a child agent from a snapshot.
  - `POST /api/optimizations/:id/revert` — unwind an accepted snapshot.
  - **Launch:** `POST /api/autooptimizer/run-cycle` launches a new optimizer
    run under the job supervisor. The same endpoint is called by the
    **Improve this agent** panel on the agent edit page — click the button,
    the dashboard spawns `xvn optimize run` (the exact same verb operators
    use), cancels are supported, and runtime/output are capped. Live progress
    streams to the `/optimizer` page via SSE (`GET /api/autooptimizer/events`)
    over a Unix socket. Start the dashboard server with
    `--autooptimizer-ipc-socket /tmp/xvn-optimizer.sock` and the optimizer CLI
    auto-connects to it. See [Optimizer Config](/docs?slug=autooptimizer-config)
    for the `autooptimizer.toml` cycle settings (evaluation windows, caps,
    memory guidance).
  - UI: the **Improve this agent** panel on the agent edit page lists a slot's
    runs and links each to a routed (non-popup) run-detail view with the
    candidate table, prompt diff, metric delta, and holdout split inline.
- **Chat rail** — optimization progress surfaces live in the unified event
  stream as `optimization_candidate_started` / `optimization_candidate_metric`
  (carrying the `split`) / `optimization_candidate_selected` /
  `optimization_completed` rows.
---

## Failure classes

`xvn optimize` returns a distinct exit code per failure class so an agent can
branch without parsing text:

| Code | Class | When |
|---|---|---|
| 10 | `OptMissingData` | Corpus query resolved to no usable training data. Use `xvn optimize explain-missing-data`. |
| 11 | `OptMissingCapability` | Capability has no optimizer signature. |
| 12 | `OptProvider` | Provider unreachable / unconfigured. |
| 13 | `OptMetric` | Objective metric failed to evaluate (e.g. unknown metric). |
| 14 | `OptValidation` | Bad capability/optimizer enum, missing corpus path, signature parse/validate error. |
| 15 | `OptPersistence` | Store write failed (migration not applied, DB error). |
| 4 | `NotFound` | `inspect`/accept against an unknown run/snapshot id. |

---

## Cross-references

- [Agents](/docs?slug=agents) — capabilities, the Improve-this-agent flow, lineage.
- [CLI Reference](/docs?slug=cli-reference) — full `xvn optimize` flag inventory + exit codes.
- [Optimizer Config](/docs?slug=autooptimizer-config) — `autooptimizer.toml` cycle settings, evaluation windows, caps, memory guidance.
- [Strategies](/docs?slug=strategies) — what a tuned child agent gets swapped into.

