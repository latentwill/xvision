# XVN QA Pass 2 — Live Alpaca + Agents / Authoring

> QA run against `https://xvnej.tail2bb69.ts.net/` on 2026-06-01.
>
> Scope covered: `Eval Runs → Start eval` live-mode dialog, `Agents → New`, `Agents → Skills`, and `Agents → Memory`.
>
> This document records observed findings from exploratory QA. It is not a product spec.

## Summary

### Confirmed issues
- **Low** — The `Agents → Skills` page is currently an empty shell (`No skills yet`) in the live dataset.
- **Low** — The `Agents → Memory` page is similarly sparse (`No patterns yet` / zeroed counters), so the new refactor surfaces are present but not yet populated.

### Things that checked out
- **Live Alpaca launch gating works**: when live mode is selected and Alpaca creds are missing, the dialog surfaces a clear inline warning and does not proceed with a live run.
- **`Agents → New`** renders the template gallery and still offers the `Paper-confirmed live trader` starter.
- **The live eval dialog** exposes the expected paper-live controls: asset, capital, bar limit, and warmup bars.
- **The start dialog** remains bounded and does not expose an open-ended daemon path.

## Findings

### 1) Live eval dialog correctly blocks on missing Alpaca credentials

**Severity:** Low  
**Area:** Eval Runs / Live Alpaca  
**Type:** Validation / safety

**Observed behavior:**
- Opening **Eval → Start eval** and switching to **live** reveals the Alpaca paper section.
- The dialog shows:
  - `Asset`
  - `Capital`
  - `Bars to run`
  - `Warmup bars`
- When `Start` is pressed without Alpaca credentials, the dialog shows:
  - `Configure Alpaca paper credentials in Settings -> Brokers before starting Live.`
- No live run is launched.

**Expected behavior:**
- Live mode should be gated by broker preflight.

**Why it matters:**
- This is the safety check the product needs for live testing.
- The dialog does the right thing here: it surfaces a clear operator action instead of allowing a silent bad launch.

---

### 2) `Agents → New` is still template- and slot-driven, not “agent types” driven yet

**Severity:** Low  
**Area:** Agents / authoring  
**Type:** Product gap / terminology

**Observed behavior:**
- The new-agent builder is still organized around templates and slots.
- The page does **not** currently expose user-facing `agent types` terminology.
- The visible copy remains centered on:
  - `template`
  - `slot`
  - `single-slot draft`

**Expected behavior:**
- If the refactor is intended to become user-facing, the new terminology should surface here.

**Why it matters:**
- This is the natural authoring entry point for the refactor.
- The current surface is still useful, but the new conceptual framing has not yet landed in the UI.

---

### 3) `Agents → Skills` and `Agents → Memory` are present but empty/sparse

**Severity:** Low  
**Area:** Agents / capabilities  
**Type:** Product maturity / data coverage

**Observed behavior:**
- `Agents → Skills` renders correctly but says `No skills yet — click + Add skill to create one.`
- `Agents → Memory` renders correctly, but all counters are `0` and there are no patterns yet.

**Expected behavior:**
- These surfaces are fine as scaffolding, but they currently provide little real content in the live dataset.

**Why it matters:**
- The scaffolding is present for the skills/capabilities refactor, but there is not yet enough live content to validate the experience end-to-end.

## Notes on evidence

### Routes / flows exercised
- `/eval-runs` → Start eval dialog → switch to live mode
- `/agents/new`
- `/agents/skills`
- `/agents/memory`

### Representative checks
- The live eval dialog was exercised without starting a live run.
- The warning text for missing Alpaca credentials was verified directly in the dialog.
- The agent authoring pages were read back from Playwright and checked for the current terminology.
