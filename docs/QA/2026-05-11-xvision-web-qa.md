# xvision web QA findings

> QA run against `https://xvnej.tail2bb69.ts.net/` and related routes on 2026-05-11.
>
> Scope covered: Home / Control Tower, Strategies, Setup, Eval Runs, Settings → Providers, Settings → Brokers, Settings → Danger zone.
>
> Note: this document records *observed findings* from exploratory QA. It is not a product spec.

## Summary

### Confirmed bugs
- **High** — Providers can be saved with an empty API key, creating a broken persisted provider.
- **High** — The Setup wizard returns `500 internal error` for a valid strategy prompt.
- **High** — The accidental broken default provider cannot be recovered from the UI.
- **Medium** — The legacy `/eval` route redirects to Home instead of a dedicated eval page.
- **Medium** — The broken provider state leaks into the Home / Control Tower surface.

### UX / accessibility concerns
- **Low** — Terminology is inconsistent across the UI: `providers`, `LLM keys`, `brokers`.
- **Low** — The danger-zone confirmation inputs are too visually similar for the risk level.
- **Low** — The Orderly broker section reads like a disabled or partial form without explicit framing.
- **Low** — The empty provider state is functional but visually weak; the primary CTA is subtle.

## Findings

### 1) Providers can be saved without an API key

**Severity:** High  
**Area:** Settings → Providers  
**Type:** Functional / validation

**Observed behavior:**
- Opening **Settings → Providers** and clicking **+ Add provider** shows a provider form.
- Clicking **Save provider** with the API key left empty still creates a persisted provider row.
- The saved row is marked as `missing` a key and becomes the default provider.

**Expected behavior:**
- The form should block submission until required fields are present, or show a clear validation error.

**Why it matters:**
- The app accepts an invalid configuration and stores a broken default provider.
- This is not just a cosmetic issue; it affects the app’s startup state and downstream flows.

---

### 2) The broken default provider cannot be recovered in the UI

**Severity:** High  
**Area:** Settings → Providers  
**Type:** Functional / recoverability

**Observed behavior:**
- After the empty save above, the default `anthropic` row remains present.
- The `Remove` action for that row is disabled.
- Adding a valid second provider does not surface any visible control to promote it to default.

**Expected behavior:**
- A healthy provider should be selectable as default, or the bad row should be editable/removable.

**Why it matters:**
- One accidental empty save can leave the workspace in a state the UI cannot fix.

---

### 3) Setup wizard crashes with a 500 on a normal prompt

**Severity:** High  
**Area:** Setup  
**Type:** Functional / backend error handling

**Observed behavior:**
- Submitting a normal prompt such as `Buy dips when trend is up` on `/setup` shows `thinking…` and then fails.
- Network request: `POST /api/wizard/chat`
- Response: `500` with body `{"code":"internal","message":"internal error"}`
- The UI only surfaces `internal: internal error`.

**Expected behavior:**
- The wizard should either succeed, or show a useful message that tells the user what failed and how to recover.

**Why it matters:**
- The onboarding path is blocked by a generic server-side failure.
- The error is too vague to debug from the UI.

---

### 4) `/eval` is a stale route that lands on Home

**Severity:** Medium  
**Area:** Routing / navigation  
**Type:** Functional / route mismatch

**Observed behavior:**
- Directly visiting `/eval` ends up at `/` Home / Control Tower.
- The actual eval page is `/eval-runs`.

**Expected behavior:**
- `/eval` should either render a dedicated eval page or clearly redirect to `/eval-runs` with a stable route contract.

**Why it matters:**
- Old bookmarks and external links to `/eval` do not work as expected.
- The sidebar label `Eval` is easy to misread as the destination route.

---

### 5) Broken provider state leaks into Home / Control Tower

**Severity:** Medium  
**Area:** Home / Control Tower  
**Type:** UX / system state visibility

**Observed behavior:**
- After the broken provider is created, Home shows a `Needs attention` item:
  - `1 provider missing API key`
  - `anthropic → ANTHROPIC_API_KEY`
- The dashboard is now visibly in a degraded configuration state.

**Expected behavior:**
- The home surface should still be truthful, but the system should avoid allowing this bad state in the first place.

**Why it matters:**
- This confirms the bad provider save is not isolated to Settings; it propagates into the main app surface.

---

### 6) Terminology is inconsistent across the settings surfaces

**Severity:** Low  
**Area:** Home / Settings / Setup  
**Type:** UX / content clarity

**Observed behavior:**
- The UI alternates between `providers`, `LLM keys`, `brokers`, and `LLM providers`.
- The setup prompt also leans heavily on `LLM key` language.

**Expected behavior:**
- One primary naming scheme should be used consistently, with secondary terms explained once if needed.

**Why it matters:**
- Users have to infer whether these are different concepts or just different labels for the same setup.

---

### 7) Danger-zone confirmation inputs are visually ambiguous

**Severity:** Low  
**Area:** Settings → Danger zone  
**Type:** UX / safety

**Observed behavior:**
- Both destructive actions require typing `DELETE`.
- The two confirmation inputs look nearly identical.
- The buttons are visually subtle for high-risk actions.

**Expected behavior:**
- Each destructive action should have stronger visual separation and clearer, action-specific confirmation text.

**Why it matters:**
- The current design is safe enough to avoid accidental clicks, but not strong enough to be ideal for destructive operations.

---

### 8) Orderly broker section looks partially implemented / read-only

**Severity:** Low  
**Area:** Settings → Brokers  
**Type:** UX / state clarity

**Observed behavior:**
- Alpaca is shown as a normal editable form.
- Orderly is shown as a status block with `unset` values and no editing affordance.
- The page does not clearly explain whether Orderly is intentionally disabled in this mode or simply unfinished.

**Expected behavior:**
- Unsupported or inactive broker sections should be explicitly labeled as such.

**Why it matters:**
- The current presentation can read as a partial UI rather than an intentional disabled state.

---

### 9) Empty provider state is functional but weakly emphasized

**Severity:** Low  
**Area:** Settings → Providers  
**Type:** UX / affordance

**Observed behavior:**
- The page shows an empty-state message and a `+ Add provider` button.
- The call to action is somewhat subtle and the page leaves a lot of unused vertical space.

**Expected behavior:**
- The primary action should be visually obvious in an empty state.

**Why it matters:**
- It is easy to miss the intended next step.

## Things that checked out

- **Brokers validation works** for empty Alpaca credentials; it does not silently save bad input.
- **Empty submission in Setup** is blocked because the Send button is disabled until text is entered.
- **Strategies → New strategy** routes to `/setup` correctly.
- **Strategies → New from template** is disabled intentionally and shows a `Coming in Plan 3 (Authoring)` tooltip.
- **Danger zone gating** works: the destructive buttons stay disabled until the correct confirmation text is entered.

## Suggested next fixes

1. Block empty provider saves and surface validation inline.
2. Allow recovery from a broken default provider state.
3. Replace the generic Setup `internal error` with actionable error handling.
4. Redirect `/eval` to the real eval route or restore a dedicated eval page.
5. Normalize terminology across Home, Setup, and Settings.

## Notes on evidence

I captured screenshots and browser state during QA, but only the written findings are committed here. If needed, I can follow up with a second pass that adds annotated screenshots or a shorter bug-triage appendix.
