# Agent CLI Helpers

These scripts are small operator helpers for agents driving a live xvision node.
They do not replace `xvn --help`; they keep common remote workflows shell-free
and make the dashboard API easier to inspect.

Default live URL:

```bash
export XVN_BASE_URL=https://xvn.tail2bb69.ts.net
export XVN_REMOTE_URL=https://xvn.tail2bb69.ts.net
```

## Use The Right Surface

| Need | Preferred surface | Helper |
| --- | --- | --- |
| Run allowlisted CLI jobs on a live node | Remote CLI job API | `scripts/xvn-remote.py` |
| Inspect dashboard JSON routes | Dashboard API | `scripts/xvn_api.py` |
| Summarize a completed eval export | Dashboard API, peer of `xvn eval export` | `scripts/xvn_eval_harness.py export-summary <run_id>` |
| Collect evidence for a broken strategy/run | Dashboard API composition | `scripts/xvn_investigate.py --strategy <id> --run <id>` |
| Validate an inline filter JSON file | Local validation first, then dashboard patch | `scripts/xvn_filter_lab.py validate <file>` |
| Create strategy/scenario records remotely | Prefer local `xvn strategy` / `xvn scenario`; dashboard API only when intentional | `scripts/xvn_author_strategy.py`, `scripts/xvn_scenario_builder.py` |
| Review memory without writing it | Dashboard memory API | `scripts/xvn_memory_report.py` |
| Capture a live SSE / streamed-event endpoint to JSONL for evidence | Dashboard SSE | `scripts/capture-sse.py` |

## Safety

- `scripts/xvn-remote.py` submits typed argv to `/api/cli/jobs`; the dashboard
  allowlist rejects mutating authoring verbs such as `strategy new` and
  `scenario create`.
- Direct API helpers can mutate dashboard state. Mutating helpers dry-run unless
  `--yes` is provided.
- The filter helper matches the shipped dashboard route: inline filters are
  attached with `PATCH /api/strategy/:id` and a `filter` field.
- `generate_strategy_template_files.py` is a local content-generation tool, not
  a live-node operator helper.
- `scripts/capture-sse.py` is read-only: it opens a stream and records events.
  It redacts secret-looking keys (`api_key`, `token`, `authorization`, …) and
  inline `sk-`/`Bearer` values before writing JSONL, so captures are safe to
  commit into an evidence ledger. Set `XVN_BEARER_TOKEN` to authenticate.

## Examples

```bash
scripts/xvn-remote.py exec --json eval list --json
scripts/xvn-remote.py events <job_id>
scripts/xvn_api.py GET /api/eval/runs
scripts/xvn_eval_harness.py export-summary <run_id>
scripts/xvn_filter_lab.py attach <strategy_id> filter.json --dry-run
scripts/capture-sse.py get /api/agent-runs/<run_id>/stream \
  --out evidence.jsonl --expect run_started,run_finished --idle-timeout 30
```
