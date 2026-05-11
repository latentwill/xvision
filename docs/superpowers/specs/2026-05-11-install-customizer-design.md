# Install Customizer — Design

> **Status:** Deferred — design accepted, implementation gated on the **xvn plugin architecture** ([F28](../../../FOLLOWUPS.md#f28-shared-plugin-architecture-for-xvn-make-optional-modules-pluggable)) landing first. Without a plugin contract there is nothing for the customizer to install. Drafted 2026-05-11.
> **Author:** xvision team
> **Companion specs:** [Marketplace Plugin](./2026-05-09-marketplace-plugin-design.md) · [Karpathy Autoresearcher](./2026-05-09-karpathy-autoresearcher-design.md) · [Cortex Memory Integration plan](../plans/2026-05-11-cortex-memory-integration-plan.md)
> **Tracking:** [F27 (this spec)](../../../FOLLOWUPS.md#f27-shared-install-customizer-interactive-module-selection-on-install--upgrade) · [F28 (plugin architecture, blocking)](../../../FOLLOWUPS.md#f28-shared-plugin-architecture-for-xvn-make-optional-modules-pluggable)

---

## 1. Purpose

`xvn install` (or first-run on a fresh deployment) currently delivers the full default build. As optional capabilities accrete — marketplace (ERC-8004 publishing), memory (cortex sidecar), autoresearcher (evening mutator loop), and the long tail of future plugins — operators want to choose what they ship. The install customizer is the interactive surface for that choice.

The customizer's job is narrow:

1. Discover available optional modules from the plugin registry (per F28).
2. Present them with one-line descriptions, dependencies, and resource cost (binary size delta, sidecar containers, extra env vars / API keys).
3. Let the operator opt modules in or out, persist the selection, and produce a reproducible install manifest.
4. Drive the actual install steps (cargo feature flags, docker-compose service composition, config scaffolding, env var prompts).
5. Be re-runnable post-install to add or remove modules without a clean reinstall.

It does **not** invent new install primitives. It composes the existing `scripts/setup_runpod.sh` stage map, cargo features, and docker-compose service overlays.

## 2. Persona & UX

**Persona.** Operator standing up a new xvn deployment, or an existing operator who wants to enable a previously-skipped module (e.g. "I want to turn on the autoresearcher now that v1 paper trading is stable").

**Surfaces.**

- **Terminal wizard** (default). `xvn install` or `xvn install --customize` drops into a TUI selector — checkbox list of modules grouped by category (core / capabilities / integrations / experimental), with descriptions and dependency arrows.
- **Non-interactive** for CI / Dockerfile / RunPod scripts. `xvn install --modules marketplace,memory,autoresearcher` or `xvn install --manifest install.toml`. Emits the same manifest the wizard writes.
- **Re-entrant.** `xvn install --add memory`, `xvn install --remove marketplace`, `xvn install --reconfigure` for env / API key updates.

**Persisted manifest.** `~/.xvn/install.toml` (or project-local equivalent) records the selected module set, version pins, and any config-only overrides. The manifest is the canonical input to subsequent builds and to the dashboard's Settings → Install pane (read-only mirror).

```toml
# ~/.xvn/install.toml
schema_version = 1
xvn_version    = "0.4.0"

[modules.marketplace]
enabled = true
version = "0.4.0"           # tracks xvn version unless pinned independently
config_ref = "marketplace.toml"

[modules.memory]
enabled = true
version = "0.4.0"
sidecar = "cortex-http"      # implies docker-compose overlay
config_ref = "memory.toml"

[modules.autoresearcher]
enabled = false              # opted out; no autoresearcher artifacts will be produced
```

## 3. Locked decisions

| # | Decision |
|---|---|
| 1 | **Customizer is opt-in re-runnable**, not one-shot. First-run defaults to the recommended preset; the operator can change selection any time without reinstalling from scratch. |
| 2 | **Recommended preset = "Persona A v1"**: core + memory off + marketplace off + autoresearcher off. Lowest-friction first-run. Matches v1-shipping-plan §"Routes NOT shipping for v1 test." |
| 3 | **Modules are discovered from the plugin registry**, not hard-coded in the customizer. Adding a new plugin = registering it; no customizer code change needed. (Depends on F28.) |
| 4 | **Manifest is the source of truth.** Build scripts (`setup_runpod.sh`, `docker-compose.yml` overlay generator), the dashboard's Install pane, and `xvn doctor` all read `install.toml`. No drift between "what the operator selected" and "what is actually compiled/running." |
| 5 | **Per-module config + env-var prompts are owned by the plugin**, not the customizer. The customizer asks each enabled plugin's manifest "what do you need from the user?" and runs that subflow. Keeps the customizer dumb and the plugins self-describing. |
| 6 | **Cargo feature flags + docker-compose overlays are emitted artifacts**, not hand-maintained. The customizer generates a `.cargo/xvn-features` file (consumed by build scripts) and a `docker-compose.override.yml` (or equivalent) from the manifest. |
| 7 | **No rollback on failed install steps in v1.** If a plugin's install step fails the customizer aborts with a clear error and leaves the manifest in its pre-attempt state. Recovery is "fix the underlying error, re-run." Atomic / transactional installs are v2. |

## 4. In scope (v1)

- Interactive TUI wizard (`xvn install` / `xvn install --customize`).
- Non-interactive flags + manifest file for CI.
- Re-entrant add / remove / reconfigure subcommands.
- Persisted `install.toml` with schema versioning.
- Cargo feature flag + docker-compose overlay generation from manifest.
- Per-plugin config / env-var subflows (driven by plugin manifest).
- Initial registered modules: **marketplace**, **memory** (cortex sidecar), **autoresearcher**.
- Dashboard Settings → Install pane (read-only mirror of `install.toml` with "Reconfigure" deep-link to the CLI).

## 5. Out of scope (deferred to v2+)

- Atomic / transactional installs with rollback.
- Plugin marketplace (discovery of community / third-party plugins from a remote registry).
- **Monetised / payment-gated plugins** — third-party plugins distributed via the marketplace and wrapped in monthly subscriptions or streamed payments (Superfluid / Sablier on Mantle), one-time mint-to-unlock, or per-cycle metered usage. Enforcement would hang off the marketplace plugin's ERC-8004 surface (receipts gate activation). v1 keeps the plugin manifest schema (per F28) carrying an optional `monetisation` field so payment-gated plugins are representable without a schema break, but the customizer ignores it.
- Auto-update of installed plugin versions (current model: plugins track xvn version).
- Multi-profile installs (dev / staging / prod manifests on the same host).
- GUI installer wrapping the CLI for non-technical operators.
- Cross-host orchestrated installs.
- Capability-graph solving for transitive plugin deps (v1 assumes a flat dep list per plugin and surfaces conflicts as errors, not auto-resolves).

## 6. Architecture

```
xvision-cli/
└── src/commands/install/        # NEW
    ├── mod.rs                   # xvn install … entry point
    ├── wizard.rs                # TUI (ratatui / inquire)
    ├── manifest.rs              # install.toml read/write/migrate
    ├── render.rs                # manifest → .cargo/xvn-features, docker-compose.override.yml
    └── reconfigure.rs           # add/remove/reconfigure flows

xvision-engine/
└── src/plugins/                 # NEW — see F28 plugin architecture spec (to be written)
    ├── mod.rs                   # PluginRegistry, PluginManifest, install hooks
    └── builtins/                # marketplace / memory / autoresearcher manifests
```

The customizer talks to the plugin registry through a small `PluginCatalog` API:

```rust
trait PluginCatalog {
    fn list(&self) -> Vec<PluginManifest>;
    fn get(&self, id: &PluginId) -> Option<PluginManifest>;
}

struct PluginManifest {
    id: PluginId,                       // "marketplace" / "memory" / "autoresearcher"
    name: String,
    description: String,                // one-line for the wizard
    long_description: String,           // shown in the detail pane
    category: PluginCategory,           // Core / Capability / Integration / Experimental
    cargo_features: Vec<String>,        // features to flip on in xvision-engine / xvision-cli
    sidecars: Vec<SidecarSpec>,         // docker-compose services to overlay
    config_template: Option<PathBuf>,   // copied to config/<plugin>.toml on enable
    env_prompts: Vec<EnvPrompt>,        // keys + helptext the wizard collects
    depends_on: Vec<PluginId>,
    conflicts_with: Vec<PluginId>,
    resource_cost: ResourceCost,        // size delta, RAM, optional API keys
    recommended_default: bool,
}
```

The catalog's data source is whatever F28's plugin registry exposes. v1 will likely ship with a static builtins list compiled in; v2 can extend to filesystem-discovered or remote-registry plugins.

## 7. Manifest schema & migration

`install.toml` is versioned via `schema_version`. v1 ships `schema_version = 1`. Future schema changes are handled by an idempotent migrator (`xvision-cli/src/commands/install/manifest.rs::migrate`) that rewrites the file in place during `xvn install` or `xvn doctor`.

Invariants:

- The manifest is the *only* persisted state about which modules are enabled. No parallel "is this on?" flags in `config/default.toml`.
- Per-plugin config files (`marketplace.toml`, `memory.toml`, etc.) hold the *settings* for an enabled plugin and are referenced by `config_ref`. Disabling a plugin keeps its config file (so re-enabling is fast); the customizer warns before deleting.

## 8. Build integration

Two emitted artifacts drive the actual build / runtime composition:

1. `.cargo/xvn-features` — a one-line file consumed by `setup_runpod.sh` and Dockerfile build args: `XVN_FEATURES="marketplace,memory,autoresearcher"`. Passed to `cargo build --features "$XVN_FEATURES"`.
2. `docker-compose.override.yml` — generated from the union of each enabled plugin's `SidecarSpec`. Memory plugin contributes the `cortex-http` service; marketplace contributes nothing at the compose layer (it's a cargo feature only); future plugins may contribute services.

Both files are gitignored. The manifest itself can be committed for reproducible deployments.

## 9. Initial module set (v1 registration)

| Module | Cargo feature | Sidecar | Config | Env prompts | Default | Notes |
|---|---|---|---|---|---|---|
| **marketplace** | `marketplace` (already exists, on by default in current build) | none | `marketplace.toml` | `MARKETPLACE_OPERATOR_PRIVATE_KEY` (optional; wallet-connect at runtime is the real opt-in) | off in v1 preset | Per existing marketplace-plugin-design spec, build-level feature gate is for size / audit builds; user-facing opt-in stays at wallet-connect. |
| **memory** | `memory` (new) | `cortex-http` | `memory.toml` | none (sidecar binds to 127.0.0.1; no external creds) | off in v1 preset | Per cortex-memory-integration-plan; sidecar overlay + new `xvision-memory` crate. |
| **autoresearcher** | `autoresearcher` (new) | none (in-process) | `autoresearcher.toml` | `AUTORESEARCHER_INTERN_PROVIDER`, `AUTORESEARCHER_BUDGET_USD_PER_NIGHT` | off in v1 preset | Per AR-1/AR-2/AR-3 plans; nightly cycle is opt-in. |

Future plugins register via F28's registry; no customizer change needed beyond the plugin shipping its manifest.

## 10. Open questions

- **Where does the plugin manifest live in the crate?** Inline `const PluginManifest` vs. `plugin.toml` at the crate root vs. derived from Cargo metadata. Defer to F28.
- **Is the wizard `ratatui` or `inquire`?** `inquire` is lighter (line-by-line prompts) and matches the existing CLI style; `ratatui` is more powerful but adds weight. Lean `inquire` for v1 unless the dependency graph view starts feeling cramped.
- **First-run detection.** Does `xvn install` run automatically on first `xvn` invocation against a fresh `~/.xvn/`, or strictly opt-in? Lean strictly opt-in — print a hint, don't auto-launch.
- **Persona A friction.** v1 preset is "everything off"; we should validate this isn't surprising. Hint in the wizard: "you can turn modules on later with `xvn install --add <name>`."

## 11. Acceptance criteria

- Fresh-host `xvn install` walks the operator through module selection and produces a working build matching the selection.
- `xvn install --add memory` on an existing install turns memory on, generates the sidecar overlay, re-runs the necessary build / compose-up step, and validates the cortex-http endpoint is reachable.
- `xvn install --remove marketplace` turns the feature off and warns about retained config / on-chain artifacts.
- CI can drive a full custom install via `xvn install --manifest install.toml` with zero TTY interaction.
- `xvn doctor` flags drift between `install.toml` and the actual running binary / compose state.
- Adding a new plugin (per F28) makes it appear in the wizard without customizer code changes.
