# Runbook: Cline runtime on a demo host

2026-06-10. How to make sure a demo host actually runs the Cline sidecar
runtime (`xvision-agentd`) for LLM-backed slots — and how to verify it,
because the eval resolver has a fallback path to legacy `LlmDispatch` that
used to be silent.

## How runtime selection works

`resolve_agent_runtime()` (crates/xvision-engine/src/api/eval.rs) picks the
runtime per eval launch:

| Condition | Effective runtime |
|---|---|
| `XVN_EMERGENCY_LLM_DISPATCH=1` (or `true`) | `llm-dispatch` (emergency rollback, loud warn) |
| Config has explicit `agent_runtime = "cline"` | `cline` (missing `XVN_AGENTD_BIN` then fails loudly at spawn — never downgrades) |
| Field absent from config, `XVN_AGENTD_BIN` set | `cline` |
| Field absent from config, `XVN_AGENTD_BIN` unset | `llm-dispatch` (FALLBACK — now logged at `warn` and recorded on the run) |
| Config missing / unparseable | `llm-dispatch` (FALLBACK — same visibility) |

Every resolution is logged under the `agent_runtime` tracing target and
persisted as a `supervisor_notes` row (role `agent_runtime`, severity `warn`
for any non-Cline resolution) on each eval run, so it shows up in
`xvn eval results --json` / the run export.

## Demo host checklist

1. **Env: sidecar binary path.** The deploy image (`Dockerfile.deploy`)
   already sets it; for any other process supervisor, set it yourself:

   ```bash
   XVN_AGENTD_BIN=/opt/xvision-agentd/dist/index.js
   ```

2. **Config: explicit runtime line.** The shipped seed config
   (`config/default.toml`, copied by the entrypoint to
   `$XVN_HOME/config/default.toml` on first boot) now carries this top-level
   line (it must appear BEFORE the first `[section]` header):

   ```toml
   agent_runtime = "cline"
   ```

   Hosts seeded before 2026-06-10 keep their existing writable config —
   add the line manually to `$XVN_HOME/config/default.toml` (or whatever
   `XVN_CONFIG_PATH` points at).

3. **Verify after a run.** Either:
   - logs: look for `agent_runtime=cline (explicit agent_runtime = "cline" in config)`
     (a `warn` mentioning `FALLBACK` means you're on llm-dispatch);
   - run record: the eval run's supervisor notes include an
     `agent_runtime` row with `{"runtime":"cline", ...}`.

## Rollback

Incident lever (process-scoped, opt-in, loudly logged):

```bash
XVN_EMERGENCY_LLM_DISPATCH=1
```

routes LLM slots back through the legacy raw-dispatch path regardless of
config. Unset it to restore Cline. See `MANUAL.md` (Emergency rollback).
