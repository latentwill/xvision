# xvn Plan Gap Answers — DSPy, Autoresearcher, and ClineSDK

> **Scope note:** This review is **not** assuming a VS Code extension. The ClineSDK report is evaluated as a runtime / trace / observability problem inside xvision.

## High-level summary

The four plan tracks are directionally good, but they are not equally grounded yet.

- **Plan 1 (data integrations / capabilities):** the biggest risk is **assuming data availability before discovery**. The revised plan should treat any exchange-adjacent or onchain sources as discovered capabilities, extend the existing Settings → Providers surface where possible, and carry cost / quota metadata instead of assuming the data layer is already solved.
- **Plan 2 (multi-agent routing):** the routing idea is sound, but the plan needs a more complete inventory of **every UI/CLI surface** that renders strategy structure, agent identity, provenance, compare output, and trace data.
- **Plan 3 (loose-framework bots):** `intern` is already a **hard-coded capability/agent class** in xvision. `Researcher` should remain a **display alias**, not a hidden new type. The UI/CLI already expose capability choice, but the plan should be explicit about where `intern` remains canonical.
- **Plan 4 (local providers):** xvision already has a provider settings surface, model catalog refresh, and a `local-candle` kind. But **llama.cpp** and **Ollama** are not first-class presets yet, and the plan needs clearer provenance / discovery boundaries.
- **ClineSDK report:** the major remaining gap is not recording; it is the **new trace as the operator surface** for correlating observability, replay, and trajectory data. The redesigned trace should be the place where the report’s observability story becomes legible.

## Plan 1 — data integrations / capabilities

### Q: Did the research identify any exchange-adjacent data sources for the plan, or is that still ungrounded?

**Answer:** it is still **not grounded as a blanket claim**.

What the docs currently support:
- some earlier planning notes explored exchange-adjacent market-data feeds as things to investigate
- the current Plan 1 should still treat such sources as **possible providers** and verify first

What is **not** established:
- that any specific exchange-adjacent source is already a stable, general-purpose data platform for the new capability surface
- that the required market / onchain / funding / OI / liquidation fields are already exposed and reliable

**Bottom line:** keep this as a discovery track, not a claim.

### Q: Do we have settings panels for the API keys?

**Answer:** **partially, yes — but only for provider configuration generally.**

Current state:
- xvision has a **Settings → Providers** surface.
- It supports provider add/update/remove, enabled model selection, connection tests, and catalog refresh.
- Providers can store API key env names and inline keys via the dashboard secret path.

But for Plan 1 specifically:
- there is **not yet a dedicated settings surface for external data providers/capabilities**
- if the new data adapters need their own credentials or rate-limit settings, that is a gap

### Q: Are API costs accounted for?

**Answer:** **not for external data APIs.**

What exists today:
- token / eval cost accounting exists in the broader system
- provider diagnostics and model catalog surfaces exist

What is missing:
- a cost model or quota model for external data provider calls
- operator-facing disclosure of API usage cost, quota, or rate-limit pressure for the new capability surface

**Implication for the plan:** add explicit cost / quota metadata if these integrations are real.

### Q: Are capabilities surfaced in the UI and CLI for the agent? (Agent page, strategy pages, CLI)

**Answer:** **yes for existing agent capability diagnostics; no for the new data capabilities yet.**

Current surfaced areas:
- **Agent page / agent inspect:** per-slot diagnostics show capability readiness and whether a capability is optimizable.
- **Strategy page / authoring:** strategy diagnostics and validation are exposed in the UI.
- **CLI:** `xvn agent inspect --diagnostics`, `xvn strategy diagnostics`, and related strategy validation verbs exist.

What is missing for Plan 1:
- a data-capability registry surfaced as a first-class UI/CLI object
- a way to ask “what data capabilities are available?” separate from LLM slot diagnostics
- explicit discovery / refresh views for provider-backed data capabilities

## Plan 2 — multi-agent routing

### Q: Are all UI and CLI surfaces accounted for?

**Answer:** **not yet.** The plan is close, but it does not fully enumerate every visible surface that will need to understand named agents and handoffs.

Already covered by the plan:
- strategy graph / pipeline shape
- strategy authoring
- stable node identity
- handoff envelopes
- trace provenance

Surfaces that should be explicitly named in the plan:
- strategy list / detail pages
- authoring page agent cards
- run detail / eval compare surfaces
- trace / inspect views that show provenance and handoffs
- CLI `strategy create`, `set-pipeline`, `show`, `validate`, and `diagnostics`
- any agent-library or diagnostics pages that display capability / role labels

**Recommendation:** add a surface inventory section so routing semantics do not diverge between UI, CLI, and trace.

## Plan 3 — loose-framework bots

### Q: Is `intern` a hard-coded agent type? How does it differ from `trader`?

**Answer:** **yes, `intern` is a real hard-coded capability/class today.**

Current behavior from the repo:
- `Capability::Intern` exists in the engine.
- `xvn agent create --capability intern` is supported in the CLI.
- diagnostics code knows `intern` as a real capability.
- the optimizer surface already mirrors `intern` as an optimizable capability target.

Difference from `trader`:
- `trader` is the active decision-making / execution-facing class.
- `intern` is evidence-preparation / research support.
- `trader` is optimizable today; `intern` is not treated the same way in diagnostics.

### Q: Are there UI and CLI surfaces to choose agent types? Is this a hidden feature?

**Answer:** **it is visible, not hidden — but the UX is uneven.**

Visible surfaces today:
- CLI `xvn agent create --capability ...` includes `intern`.
- strategy authoring already works with agent roles and capability selection.
- diagnostics surfaces show the declared capabilities and readiness.

What is still missing / confusing:
- the UI does not yet read like a clear “agent type chooser” in the loose-bot sense
- `Researcher` vs `Intern` naming needs an explicit policy so it doesn’t look like two different runtime concepts
- if the product wants a user-facing “Researcher” label, it should be a **display alias only**, with `intern` remaining canonical underneath

### Proposed clarification for the plan

- Keep `intern` as the engine/CLI canonical capability.
- Let `Researcher` be a UI/prompt alias.
- Do not invent a second hidden agent class.

## Plan 4 — local llama.cpp and Ollama providers

### Q: Do we have a settings panel selector for this?

**Answer:** **we have a provider settings panel, but not explicit llama.cpp / Ollama presets yet.**

Current state:
- Settings → Providers exists.
- The provider form includes a kind selector with `anthropic`, `openai-compat`, and `local-candle`.
- Provider rows support key envs, inline secrets, enabled models, model refresh, and connectivity checks.

Missing for Plan 4:
- explicit presets for `llama.cpp`
- explicit presets for `Ollama`
- provenance / discovery UX tailored to local runtimes

### Q: Are provider settings and model select / discovery built in?

**Answer:** **yes, partially.**

Built in today:
- provider CRUD
- API key env / inline secret support
- model enablement
- model catalog refresh
- provider-level connection test

What is missing for the new local providers:
- a provenance model that distinguishes verified, unverified, and unknown model identity
- discovery semantics for local runtimes that may not expose a stable model digest
- a marketplace-facing disclosure object for non-standard or locally hosted models

### Q: How about with Candle?

**Answer:** Candle already exists as `local-candle`, but it is not the same thing as the new local-provider work.

Current Candle situation:
- Candle is already a first-class kind in the provider settings surface.
- `base_url` is ignored for Candle because it is in-process.
- It gives xvision a local-provider path today.

Gap relative to the new plan:
- Candle does not solve the provider provenance / digest / discovery questions that Plan 4 is targeting for Ollama and llama.cpp.
- The plan should preserve Candle as-is while adding new local runtime presets and provenance metadata.

## ClineSDK report — where the real gap is

The report should be treated as a **trace projection problem**, not a missing-recording problem.

What xvision already has:
- observability plumbing
- trajectory recording / replay artifacts
- export surfaces
- trace-dock chrome

What is still missing:
- a unified operator experience that correlates the observability ledger, trajectory store, and replay metadata
- clear terminal-state semantics in the redesigned trace
- a strong answer to “what exactly happened in this run?” without jumping across multiple screens

### Practical takeaway

The next xvn plan should focus on:
- projecting observability into the redesigned trace
- using the trace as the operator-facing review surface
- keeping SQLite / trajectory storage canonical underneath
- avoiding any VS Code extension dependency in the scope

## Recommended next steps

1. Revise Plan 1 to use the existing provider settings surface, add visible capability discovery/refresh, and carry cost / quota metadata.
2. Expand Plan 2 with an explicit UI/CLI/trace surface matrix.
3. Clarify Plan 3 so `Researcher` is only a display alias over canonical `intern`.
4. Extend Plan 4 with explicit local-provider presets and provenance states.
5. Prioritize the redesigned trace as the observable surface for the ClineSDK gap.
