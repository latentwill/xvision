# Plan 4 — Local llama.cpp and Ollama Providers for xvision

> **For Hermes:** use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this task-by-task.
>
> **Status:** Draft implementation plan.

**Goal:** Add *first-class* local model providers for xvision — specifically **llama.cpp** and **Ollama** — so the product is not limited to Candle. At the same time, add marketplace-facing strategy metadata that can disclose when a strategy uses a **non-standard model** and, when possible, a verifiable model digest / hash surface.

**Core constraint:** Separate *provider capability* from *model provenance / verification*.
- A provider can be local, OpenAI-compatible, and still have very different provenance surfaces.
- A model can be launchable even when no digest is available.
- Missing verification data must degrade to an explicit `unverified` / `unknown` state, not a hard failure.

**Key discovery question:** Does Ollama expose a stable model digest or similar verification surface that we can persist and display?
- Likely candidates to inspect: the Ollama registry/list endpoints and the model detail endpoint (`/api/tags`, `/api/show`, or equivalent in the version we target).
- If a digest is present, persist it as a provider-native verification surface.
- If no digest is available, keep launchability intact and surface `unverified` with a clear reason.
- For llama.cpp, use a digest only when the operator supplies a locally-addressable artifact path or an equivalent verifiable surface; otherwise keep the model as unverified.

---

## Architecture shape

This plan treats provider support as a layered problem:

- **Transport kind** — Anthropic, OpenAI-compatible, or local runtime.
- **Provider preset / flavor** — Ollama, llama.cpp, Candle, generic OpenAI-compat.
- **Capability set** — chat/completions, catalog discovery, model digest support, local-file verification, keyless auth.
- **Model provenance** — a structured disclosure attached to a model entry / strategy summary.
- **Marketplace disclosure** — a structured UI-level object with separate axes for model class, provenance source, and verification status.

The implementation should keep the existing provider/model override flow, but extend the data model so the UI and strategy summary can distinguish:
- standard remote model
- local but unverified model
- local model with a provider-registry digest
- local model with a file hash
- non-standard model without any reliable hash surface

**Important distinction:** transport kind remains the low-level provider path; presets/flavors are the user-facing selections that map onto it. That keeps local-provider UX from turning into a new transport taxonomy.

---

## File map: exact files to touch

### Core / engine / provider plumbing
- Modify: `crates/xvision-core/src/config.rs`
- Modify: `crates/xvision-core/src/providers/mod.rs`
- Modify: `crates/xvision-core/src/providers/catalog.rs`
- Create: `crates/xvision-core/src/providers/provenance.rs`
- Modify: `crates/xvision-core/src/providers/model_metadata.rs`
- Modify: `crates/xvision-engine/src/providers/mod.rs`
- Modify: `crates/xvision-engine/src/providers/fetcher.rs`
- Modify: `crates/xvision-engine/src/providers/cache.rs`
- Modify: `crates/xvision-engine/src/providers/service.rs`
- Re-export: `crates/xvision-engine/src/providers/provenance.rs`
- Modify: `crates/xvision-engine/src/api/settings/providers.rs`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Modify: `crates/xvision-engine/src/strategies/manifest.rs`
- Possibly modify: `crates/xvision-engine/src/strategies/slot.rs` if the slot label needs to carry provenance/disclosure text

### Frontend / dashboard / marketplace surfaces
- Modify: `frontend/web/src/routes/settings/providers.tsx`
- Modify: `frontend/web/src/routes/settings/providers.test.tsx`
- Modify: `frontend/web/src/routes/strategies.tsx`
- Modify: `frontend/web/src/routes/strategies.test.tsx`
- Modify: `frontend/web/src/routes/authoring.tsx`
- Modify: `frontend/web/src/routes/authoring-risk.tsx`
- Modify: `frontend/web/src/routes/authoring-risk.test.tsx`
- Modify: `frontend/web/src/routes/eval-runs.tsx`
- Modify: `frontend/web/src/routes/eval-runs.test.tsx`
- Modify: `frontend/web/src/features/marketplace/data/types.ts`
- Modify: `frontend/web/src/features/marketplace/routes/CreatorRoute.tsx`
- Modify: `frontend/web/src/features/marketplace/routes/LineageRoute.tsx`
- Possibly modify: `frontend/web/src/features/marketplace/routes/browse/ListingCard.tsx` if the marketplace list should carry the same disclosure chip

### Docs / test fixtures
- Modify or add tests next to every touched module above
- Create or update any fixture JSON/TOML needed for local-provider examples

---

## Implementation plan

### Task 1: Define provider capability and model provenance types

**Objective:** Give xvision a structured way to say *what kind of provider this is* and *what can prove the model identity*.

**Files:**
- Modify: `crates/xvision-core/src/config.rs`
- Modify: `crates/xvision-core/src/providers/catalog.rs`
- Create: `crates/xvision-core/src/providers/provenance.rs`
- Modify: `crates/xvision-core/src/providers/mod.rs`
- Modify: `crates/xvision-engine/src/api/settings/providers.rs`
- Modify: `crates/xvision-engine/src/providers/provenance.rs`

**Work:**
- Extend provider modeling so local runtime flavors are explicit, not just “custom openai-compat”.
- Add provider flavors/presets for:
  - `ollama`
  - `llama-cpp`
  - keep `local-candle`
  - keep generic `openai-compat`
- Add a structured provenance payload for model entries and strategy summaries, with optional fields for:
  - `verification_kind` / `verification_source`
  - `digest_algorithm`
  - `digest_value`
  - `registry_ref` or model registry label
  - local artifact path / file hash when available
  - `verification_status` (`verified`, `unverified`, `unknown`, or similar)
- Keep all provenance fields optional so legacy catalogs and strategies deserialize cleanly.
- Make the capability model clear that a provider can support chat/completions without supporting a verifiable model digest.

**Implementation note:**
- Do **not** bake provider provenance into the existing `kind` string alone. `kind` should stay transport-oriented; provenance belongs in a separate structured field.
- Keep the new provenance type serde-friendly and TS-exportable where needed.

**Verification surface decision:**
- Ollama: prefer registry/provided digest if the API returns one, but treat it as best-effort metadata unless it is clearly artifact-bound.
- llama.cpp: use a digest only if the operator can point to a local artifact or the server exposes a trustworthy artifact reference.
- Generic OpenAI-compatible providers: provenance usually remains absent.

**Tests:**
- Round-trip serde tests for the provenance structs.
- Backward-compatibility test that old provider rows and catalogs still deserialize.
- Unit test that missing provenance stays `None` / `unknown`, not `error`.

---

### Task 2: Add first-class local provider presets and routing

**Objective:** Make Ollama and llama.cpp available as explicit provider choices, not hidden behind a generic custom field.

**Files:**
- Modify: `frontend/web/src/routes/settings/providers.tsx`
- Modify: `frontend/web/src/routes/settings/providers.test.tsx`
- Modify: `crates/xvision-core/src/config.rs`
- Modify: `crates/xvision-engine/src/providers/fetcher.rs`
- Modify: `crates/xvision-engine/src/providers/service.rs`
- Modify: `crates/xvision-engine/src/providers/cache.rs`

**Work:**
- Add UI provider presets for:
  - Ollama
  - llama.cpp
  - keep a generic custom OpenAI-compatible preset for self-hosted variants
- Default local presets to no-key, localhost-friendly URLs.
- Route Ollama and llama.cpp through the OpenAI-compatible chat/catalog path unless a provider-specific endpoint needs special handling.
- Add explicit provider-family detection so the backend can attach the right provenance behavior and not confuse local providers with generic OpenAI-compatible remote services.
- Keep current Candle behavior intact.
- Ensure catalog fetches for local providers are safe and non-blocking; if a provider does not expose a usable catalog, the UI should still allow manual model binding.

**Discovery task for Ollama:**
- Inspect the registry/listing endpoint(s) we can use against the supported Ollama versions.
- Record whether the API exposes:
  - a stable `digest`
  - a model `hash`
  - any registry artifact identifier that can serve the same purpose
- If a digest exists, preserve it verbatim in model metadata.
- If not, keep the provider launchable and mark provenance as unverified.

**Tests:**
- Provider preset render tests in `settings/providers.test.tsx`.
- HTTP-mocked fetcher tests for Ollama / llama.cpp local endpoints.
- Regression test that generic OpenAI-compatible providers still route exactly as before.

---

### Task 3: Extend catalog/model metadata to carry provenance

**Objective:** Let the model catalog answer “what is this model, and how can I prove it?” without inventing fake certainty.

**Files:**
- Modify: `crates/xvision-core/src/providers/catalog.rs`
- Create: `crates/xvision-core/src/providers/provenance.rs`
- Modify: `crates/xvision-core/src/providers/model_metadata.rs`
- Modify: `crates/xvision-engine/src/providers/fetcher.rs`
- Modify: `crates/xvision-engine/src/providers/cache.rs`

**Work:**
- Add provenance fields to `ModelEntry` or attach a parallel provenance object keyed by model id.
- Preserve existing `ModelEntry` behavior for providers that only expose an id.
- For Ollama, attach any returned digest / registry reference.
- For llama.cpp, attach a local-file digest only when a verifiable file path or checksum is available.
- Make cache serialization resilient to older cache files by versioning fields or using optional additions only.
- Keep the model metadata lookup fallback behavior intact for old ids and unknown models.

**Decision rule:**
- `standard` model + no digest = still acceptable for launch, but marketplace disclosure should say “unverified / no digest”.
- `local` model + digest = “verified local model” only if the digest is artifact-bound; otherwise still disclose the digest as best-effort metadata.
- `non-standard` model with no digest = “non-standard, unverified”.

**Tests:**
- Model-entry serde round-trip.
- Cached-catalog migration test for old JSON files.
- Ollama parse test that captures digest when present and leaves it empty when absent.
- llama.cpp parse test for the no-hash fallback path.

---

### Task 4: Add strategy-facing metadata for non-standard model disclosure

**Objective:** Make strategy summaries and manifests capable of telling the marketplace that a strategy uses a non-standard model, and whether that model has a verifiable digest.

**Files:**
- Modify: `crates/xvision-engine/src/strategies/manifest.rs`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Possibly modify: `crates/xvision-engine/src/strategies/slot.rs`
- Modify: `frontend/web/src/routes/strategies.tsx`
- Modify: `frontend/web/src/routes/strategies.test.tsx`
- Modify: `frontend/web/src/routes/authoring.tsx`
- Modify: `frontend/web/src/routes/authoring-risk.tsx`
- Modify: `frontend/web/src/routes/authoring-risk.test.tsx`

**Work:**
- Add marketplace-facing metadata to the strategy manifest / summary to distinguish:
  - standard model
  - local model
  - non-standard model
  - verified digest present
  - digest absent / unverified
- Keep authored claims and runtime-captured provenance separate: manifest fields are the user’s declaration, summary/eval fields are the snapshot of what actually ran.
- Keep this metadata informational, not gating.
- Use structured fields, not only free-text `attested_with`, so the marketplace can filter and label consistently.
- Preserve backward compatibility for existing strategies that only have provider/model strings.
- Thread provenance into the strategy summary emitted by `api/strategy.rs` so list/detail routes can label it without additional lookups.
- Ensure authoring surfaces can show the disclosure before publish/save, so operators know what the marketplace will show.

**Suggested shape:**
- A compact `model_provenance` object in the strategy manifest/summary.
- A boolean / enum for `non_standard_model` or `model_disclosure_status`.
- An optional `model_digest` field for the published summary.

**Tests:**
- Summary serialization test that includes non-standard-model disclosure.
- Legacy strategy deserialization test with no provenance fields.
- UI test that strategy cards show a “local”, “non-standard”, or “verified digest” chip when appropriate.

---

### Task 5: Label the UI clearly everywhere users make trust decisions

**Objective:** Make it obvious when a strategy uses a local or non-standard model, and whether that model was verified.

**Files:**
- Modify: `frontend/web/src/routes/settings/providers.tsx`
- Modify: `frontend/web/src/routes/strategies.tsx`
- Modify: `frontend/web/src/routes/eval-runs.tsx`
- Modify: `frontend/web/src/features/marketplace/data/types.ts`
- Modify: `frontend/web/src/features/marketplace/routes/CreatorRoute.tsx`
- Modify: `frontend/web/src/features/marketplace/routes/LineageRoute.tsx`
- Possibly modify: `frontend/web/src/features/marketplace/routes/browse/ListingCard.tsx`

**Work:**
- In Settings → Providers:
  - show a `Local` / `Ollama` / `llama.cpp` label instead of burying them under generic custom text
  - show a provenance hint if the provider can expose a digest
- In Strategies:
  - show a chip/pill for `local`, `non-standard`, and `verified digest` status
  - keep the existing provider/model summary but add a second line or badge row for provenance
- In eval launch / eval results:
  - display the selected model’s provenance status
  - show “unverified” explicitly when no digest exists
- In marketplace-facing surfaces:
  - carry the disclosure into card/list/detail views
  - keep the trust language consistent with existing `VerifiedBadge` / `unverified` semantics

**UI copy guidance:**
- Prefer neutral labels:
  - `Local model`
  - `Non-standard model`
  - `Verified digest`
  - `Unverified`
- Avoid implying that a missing digest is an error.
- Avoid calling a local model “verified” unless a real digest or equivalent surface exists.

**Tests:**
- Settings provider preset and label tests.
- Strategy list/detail label tests.
- Marketplace route tests for badge rendering.
- Eval-runs test to ensure the disclosure copy is shown and not regressed to generic provider/model text.

---

### Task 6: Make eval and clone flows preserve provenance without becoming brittle

**Objective:** Ensure provenance follows the strategy through eval/clone operations, but does not block execution.

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-engine/src/strategies/slot.rs`
- Modify: `crates/xvision-engine/src/strategies/manifest.rs`
- Modify: `crates/xvision-engine/src/providers/service.rs`

**Work:**
- Ensure eval launch stores the provenance snapshot alongside the run where possible.
- If a strategy is cloned with a model override, re-resolve provenance for the override target.
- If provenance cannot be resolved, continue with explicit `unverified` / `unknown` disclosure.
- Keep provider/model validation focused on launchability, not verification.
- Do not fail eval just because the provider cannot provide a hash.
- Make sure compare / export surfaces can carry the provenance fields forward.

**Important behavioral rule:**
- **Launchability is not verification.** A model can be launchable and still unverified.
- **Verification is not a prerequisite for local usage.** It is only a disclosure / reproducibility surface.

**Tests:**
- Eval override tests with and without provenance.
- Clone tests that ensure override changes do not silently keep stale digest metadata.
- Export / compare tests if the provenance field is included in the run payload.

---

## Verification plan

### Automated tests to add or update
- `crates/xvision-core/src/providers/catalog.rs`
  - provenance serde round-trip
  - old catalog compatibility
- `crates/xvision-core/src/providers/provenance.rs`
  - digest/verification enum tests
- `crates/xvision-engine/src/providers/fetcher.rs`
  - Ollama digest capture
  - Ollama no-digest fallback
  - llama.cpp no-hash fallback
- `crates/xvision-engine/src/providers/cache.rs`
  - cache round-trip with provenance fields present
- `crates/xvision-engine/src/api/strategy.rs`
  - strategy summary includes disclosure fields
- `crates/xvision-engine/src/api/eval.rs`
  - eval launch preserves disclosure and does not reject missing digest
- `frontend/web/src/routes/settings/providers.test.tsx`
  - preset labels and digest hint visibility
- `frontend/web/src/routes/strategies.test.tsx`
  - local/non-standard/verified badges
- `frontend/web/src/routes/authoring-risk.test.tsx`
  - authoring preview shows provenance disclosure
- `frontend/web/src/routes/eval-runs.test.tsx`
  - eval detail surfaces provenance correctly
- `frontend/web/src/features/marketplace/...` tests
  - marketplace cards/detail views show the disclosure without breaking verified/unverified styling

### Manual verification checklist
- Add an Ollama provider in the UI and confirm the catalog/model list loads.
- Add a llama.cpp provider in the UI and confirm the chat/completions path still works.
- Confirm the UI shows a digest when Ollama exposes one.
- Confirm the UI shows `unverified` when no digest is available.
- Confirm an eval run still launches when provenance is missing.
- Confirm the marketplace-facing strategy detail labels the model as local/non-standard when appropriate.

---

## Risks

- **Ollama API variance:** different Ollama versions may expose digest data in different endpoints or field names.
- **False confidence:** a digest-like string from a provider may not represent the exact artifact we care about.
- **llama.cpp hash gap:** many llama.cpp deployments do not expose a trustworthy artifact path; verification may remain unavailable.
- **Back-compat churn:** adding provenance fields to catalogs and summaries can affect old cache files and generated TS types.
- **UI over-labeling:** it is easy to accidentally imply “unverified” means “broken”; copy should stay neutral.
- **Eval gating drift:** verification should stay informational; do not accidentally start rejecting local providers due to missing digest.

---

## Acceptance criteria

- Users can configure **Ollama** and **llama.cpp** as first-class providers in Settings.
- Local providers behave as launchable providers even when no digest/hash is available.
- If Ollama exposes a digest or equivalent, xvision captures and displays it.
- If no digest is available, xvision clearly marks the model as **unverified** instead of inventing provenance.
- Strategy summaries and marketplace-facing views can flag **non-standard models**.
- The disclosure survives clone/eval flows and is visible in list/detail/compare surfaces.
- Existing Candle and generic OpenAI-compatible provider behavior remains unchanged.
- All new paths are covered by tests, including the “digest absent” fallback.

---

## Suggested implementation order

1. Add provider/provenance types and wire the serde/TS surface.
2. Add Ollama and llama.cpp presets and the provider-specific fetch behavior.
3. Extend catalogs with provenance and capture any digest/hashes.
4. Add strategy summary / manifest disclosure fields.
5. Add UI badges and marketplace-facing labels.
6. Add eval/clone propagation and verification tests.
7. Run the new tests plus a manual local-provider smoke test.

---

## Open question to resolve during implementation

Should llama.cpp verification be:
- **path-based** only when the operator points at a local GGUF file, or
- **server-introspected** when the endpoint exposes enough metadata to hash the active artifact?

Recommendation: support both where possible, but treat path-based hashing as optional and never required for launchability.
