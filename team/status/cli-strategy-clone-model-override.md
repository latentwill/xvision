---
track: cli-strategy-clone-model-override
worker: claude-opus
phase: ready-for-review
last_update: 2026-05-23
---

# Status

Claimed 2026-05-23. Salvaged tests in
`team/notes/2026-05-22-cleanup/salvage/...` reviewed; they target a
`Strategy.cloned_from` direct field which is **outside** the contract's
allowed_paths (`strategies/mod.rs` is not listed). Per the contract's
"extend `Strategy.metadata` JSON (preferred — no migration)" guidance,
provenance lands inside `mechanical_params.metadata.cloned_from`.

## Implementation choice

- `cloned_from` provenance stashed in
  `strategy.mechanical_params.metadata.cloned_from` (JSON object — no
  schema change, no migration, no Strategy struct touch). Salvaged
  test fixtures adapted to read from that key.
- Engine helper: new `api::strategy::clone_strategy_full(ctx, source_id,
  req) -> ApiResult<CloneStrategyFullOut>` alongside the existing
  shallow `clone_strategy` (dashboard route still consumes the shallow
  path). A new `CloneStrategyFullReq` struct carries the optional
  `provider/model` override + `display_name`. The existing
  `CloneStrategyReq` shape is preserved unchanged so the dashboard
  binding in `crates/xvision-dashboard/src/routes/strategies.rs` keeps
  compiling without a touch.

## Approach

1. Engine: `clone_strategy_full` + `CloneStrategyFullReq` +
   `CloneStrategyFullOut` (api/strategy.rs).
2. CLI: rewrite the existing `clone` body in
   `commands/strategy.rs` to delegate to the engine helper; fix the
   `--json` contract so banners go only to stderr and the JSON object
   is the only stdout output in JSON mode.
3. Tests: salvaged fixtures copied to target paths, adapted for the
   `mechanical_params.metadata.cloned_from` location and for the
   current `Strategy` / `AgentRef` / `PublicManifest` field set.
4. Verify, open PR.

## Verification

- `cargo test -p xvision-engine --test strategy_clone_model` →
  5/5 passing (unreachable provider, disabled model, half-pair,
  verbatim copy with cloned_from, override rebind).
- `cargo test -p xvision-cli --test strategy_clone_cli` → 5/5 passing
  (override creates paired agent, verbatim copy, refuses unreachable
  provider, half-pair clap surface, missing --name).
- `cargo test -p xvision-cli --test strategy_cli` → 12/12 (pre-existing
  prototype clone coverage still passes through the refactor).
- `cargo build -p xvision-dashboard` → clean (the dashboard binding to
  the unchanged `CloneStrategyReq` continues to compile).

Two CLI test binaries (`strategy_validate_warnings`,
`strategy_add_filter`) fail to build on `origin/main` itself —
pre-existing breakage from concurrent merged work (`scope_strategy_id`
on `CreateAgentRequest`, `color` on `PublicManifest`) that is outside
this contract's allowed_paths. PR notes call this out; the fix belongs
on the owning track, not here.
