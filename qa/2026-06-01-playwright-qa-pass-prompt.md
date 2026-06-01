# XVN UI QA pass — agent prompt

A self-contained brief for a Playwright/webwright agent (and its sub-agents)
to exhaustively QA the xvision dashboard SPA. Designed to be fed verbatim
to a dispatcher agent that will fan out parallel sub-agents per route
family, run two full sweeps (functional, then usability), and emit one
merged markdown report ready to feed back into fix-agents.

---

## ROLE

You are the **QA dispatcher** for the xvision dashboard SPA. You do not
fix anything. You drive a browser, observe, and produce findings dense
enough that another agent can act on each one without re-discovering it.

You will run **two sweeps in order**:

1. **Functional sweep** — does it work? (broken, missing, errored, dead.)
2. **Usability sweep** — is it good? (confusing, ugly, inconsistent, hidden.)

Each sweep dispatches one sub-agent per **route family** (listed below).
Sub-agents run in parallel. You merge their outputs into the single
report defined under OUTPUT FORMAT.

---

## TARGET

- **Base URL:** `<FILL IN STAGING URL>`
- **Auth:** `<FILL IN: cookie / login creds / bypass token>`
- **Viewport defaults:** 1440×900 desktop. Re-check every route at
  390×844 (iPhone) during the usability sweep — the dashboard has a
  mobile shell (`MListCard`, `MListSheet`) and a separate phone
  breakpoint.
- **Browser tool:** Playwright (preferred) or the Claude-in-Chrome MCP.
  Whichever you use, you MUST capture: (a) console errors, (b) network
  4xx/5xx, (c) a screenshot per finding, (d) the URL at time of finding.

---

## PROJECT CONVENTIONS — VERIFY AGAINST THESE

These are hard rules in `CLAUDE.md`. Every violation is a finding.

### Layout shell

- Desktop shell is **three-pane**: left sidebar (220px) · center column
  (flex) · right chat rail (auto). Any detail page that renders a
  **fourth column / right sidebar / floating side card** while the chat
  rail is visible is a **violation** (severity: P2 minimum, P1 if it
  squeezes the chart/decisions area).
- Auxiliary boxes (META strips, run config, persona pickers, review
  panels) belong **inline above/below** the center content, ideally as
  a horizontal chip row — not as a `col-span-4` sidebar.

### No popups

- No modals, sheets, popovers, dialogs, or focus-stealing overlays
  anywhere except: (1) toasts, (2) native browser primitives (file
  picker, print), (3) `MListSheet` on the phone breakpoint only.
- Anything that paints over the primary surface to confirm, configure,
  reveal a detail, or recover from an error is a **violation**
  (severity: P2 minimum).

### Terminology (operator-facing surfaces)

The operator never sees these words anywhere in UI labels, copy,
tooltips, empty states, or error messages:

- ~~setup~~ → **cycle** (table: `cycles`, id: `cycle_id`)
- ~~setups_evaluated~~ → **cycles_evaluated**
- ~~StrategyBundle~~, ~~bundle~~ → **Strategy**
- `Mutation` → **Experiment**
- `Mutator` → **Experiment writer**
- `LineageStatus::Ghost` → **Rejected**
- `LineageStatus::Quarantined` → **Suspect**
- `CycleSeal` → **Evening summary**
- `Merkle root` → **Cycle proof**
- `--gate-epsilon` → **min-improvement**
- `--parent-holdout-score` → **baseline-untouched-score**
- null-result canary → **honesty check**
- Cryptographic primitives (`BLAKE3`, `Ed25519`, "merkle", "canonical
  JSON") must **never** appear on an operator surface.

**Exception:** the `xvn setup` CLI verb (config init) is NOT renamed.
Seeing "setup" on the `/setup` route or in references to the CLI verb
is fine. Seeing it as a synonym for "cycle" anywhere is a finding.

---

## ROUTE FAMILIES (dispatcher fans out one sub-agent per family)

Route source of truth: `frontend/web/src/routes.tsx`. Sub-agents must
read it before starting to confirm no surface was added since this
prompt was written.

| # | Family | Routes |
|---|---|---|
| 1 | **Entry & ops** | `/login`, `/`, `/live/:id`, `/setup`, `/safety` |
| 2 | **Strategies** | `/strategies`, `/strategies-folder`, `/strategies/new`, `/strategies/:id`, `/strategies/:id/diagnostics`, `/authoring`, `/authoring/:id` |
| 3 | **Agents** | `/agents`, `/agents/new`, `/agents/:id`, `/agents/:id/flywheel`, `/agents/:id/diagnostics`, `/agents/:id/optimizations/:runId`, `/agents/memory`, `/agents/skills` |
| 4 | **Scenarios** | `/scenarios`, `/scenarios/new`, `/scenarios/:id` |
| 5 | **Eval & agent runs** | `/eval-runs`, `/eval-runs/:runId`, `/eval-runs/compare`, `/agent-runs/:runId` |
| 6 | **Marketplace** | `/marketplace`, `/marketplace/leaderboard`, `/marketplace/leaderboard/:sliceId`, `/marketplace/lineage/:name`, `/marketplace/creator/:handleOrAddr`, `/marketplace/sell`, `/marketplace/receipts/:tx` |
| 7 | **Autoresearch** | `/autoresearch`, `/autoresearch/diff/:hash` |
| 8 | **Settings & docs** | `/settings/general`, `/settings/providers`, `/settings/brokers`, `/settings/danger`, `/docs` |
| 9 | **Charts (production)** | `/charts/overview`, `/charts/compare`, `/charts/annotated`, `/charts/hero` |
| 10 | **Chart-lab (devtool — lower priority)** | `/chart-lab/*` (overview, primitives, surfaces, tokens, dashboards/*) |

For routes with `:id` params, the sub-agent picks **at least three**
real ids: a populated one, an empty/just-created one, and a stale/old
one. Capture each id used so findings are reproducible.

---

## SWEEP 1 — FUNCTIONAL

For every route in your family, do **all** of the following. Each
failure is one finding.

### A. Load-time

- Console: any `error`, any unhandled promise rejection.
- Network: any 4xx/5xx, any request that 30s+ without resolving, any
  request that fires repeatedly in a loop.
- Render: route reaches a stable state (no infinite spinner, no
  perpetual skeleton, no blank screen after suspense resolves).
- Deep-link works directly (paste the URL fresh — don't only arrive
  via in-app nav).
- Refresh works (hit F5 from the route, state recovers cleanly).
- Back/forward works (browser back from this route, then forward,
  doesn't corrupt state).

### B. Interactive elements

Click/tab through **every** visible interactive element:

- Every button does something observable (network request, navigation,
  state change, toast). A button that does nothing on click is a
  finding.
- Every link goes somewhere live (no 404, no `#`, no broken anchor).
- Every form: submit with valid input, submit with empty input, submit
  with invalid input. Validation must be visible and recoverable.
- Every dropdown/select opens, reveals options, closes on outside
  click, closes on Esc.
- Every toggle/switch persists across reload (if state is meant to
  persist).
- Every "Copy" button actually writes to clipboard.
- Every filter / search / sort actually filters/searches/sorts the
  displayed data.
- Every pagination control advances/retreats.
- Every tab in a tabbed surface loads its content.
- Every accordion expands and collapses.
- Every "Reload" / "Refresh" affordance triggers a refetch.

### C. States

For each meaningful surface on the route:

- **Empty state**: artificially induced (new account, filtered to zero
  results, just-created entity). Is there one? Is it useful (explains
  what's missing, offers next action)?
- **Loading state**: is there a skeleton/spinner, or does the page
  flash unstyled content?
- **Error state**: kill the relevant API (DevTools → block request) or
  pass a bogus id. Does the UI degrade gracefully or crash the route?
- **Stale state**: leave the tab in the background 10 minutes, return.
  Does it refetch / show "stale" indicator / silently lie?

### D. Cross-route integrity

- Sidebar nav: every item in the left rail leads to a real route, and
  the active item is highlighted correctly when you're on it.
- Breadcrumbs (where present) match the actual nav path.
- "Open in chat" / chat rail context: the right rail's context (if it
  reflects the current page) actually updates when you switch routes.
- Deep links from one surface to another (e.g. strategy → its eval
  runs, agent → its flywheel) resolve.

### E. SPA chrome

- Login flow: log out, hit a protected route, get redirected to
  `/login`, log back in, get redirected to the originally requested
  route (not always `/`).
- 404 / wildcard route: deliberately mistype a URL — should redirect
  to `/`, not crash.
- Chunk reload: simulate stale-bundle-after-deploy (block one lazy
  chunk request) — `attemptChunkReload` should fire, not crash to the
  global error UI.

---

## SWEEP 2 — USABILITY

Same routes, second visit. For every route, write findings (even if
small) in any of these dimensions:

### F. Information hierarchy

- Is the most important thing on this page actually the visually
  loudest thing? If not, what's competing?
- Can a new operator answer "what is this page for?" within 5 seconds
  of landing?
- Are key numbers (P&L, scores, counts) scannable at a glance, or
  buried in body copy?

### G. Discoverability

- Is every action that exists also discoverable from the page itself
  (not only via deep-link or keyboard shortcut)?
- Are related surfaces linked (strategy ↔ agents it composes ↔ runs
  using it ↔ scenarios paired with it)?
- Are there features mentioned in `MANUAL.md` /
  `docs/operator/` that have no UI surface? List them as **missing**
  findings.

### H. Consistency

- **Spacing**: same vertical rhythm across cards / list items / detail
  sections?
- **Type**: same scale across labels, values, captions? Same
  alignment?
- **Color**: tokens (`text-1`, `text-2`, `text-3`, `border-1`,
  `bg-1`, etc.) used consistently for the same semantic role?
- **Component reuse**: two list pages that look 70% the same but were
  clearly built separately = consistency finding.
- **Iconography**: same icon means the same thing everywhere?
- **Empty/loading/error states**: same visual language across
  surfaces?
- **Naming**: does `/eval-runs` say "Run" and `/agent-runs` say
  "Execution"? Same concept, different words = finding.

### I. Feedback

- After every action (save, run, delete, copy), does the user get
  acknowledgement within ~300ms?
- Are long-running operations (eval runs, fetches) given progress, or
  do they vanish into the void?
- Is destructive action confirmed (inline, NOT via modal)?

### J. Density & scanability

- Tables: are sortable columns marked? Is the sort direction visible?
  Is the row count visible? Can you tell what's filtered out?
- Lists: are there too many or too few items per screen? Does the
  card height vary jarringly?
- Forms: are required fields marked? Are field groupings logical? Is
  the submit button always visible?

### K. Project-convention adherence (recheck)

- Any popup / modal / sheet / popover outside the allowed exceptions?
- Any right-side box on a chat-rail route?
- Any banned terminology on an operator surface?
- Any cryptographic primitive (`BLAKE3`, `Ed25519`, "merkle") visible
  to the operator?

### L. Mobile (390×844)

- Does the page collapse into the mobile shell, or does it just look
  broken at narrow width?
- Are tap targets ≥44px?
- Does the right chat rail get hidden / docked / replaced
  appropriately?
- Do `MListCard` / `MListSheet` render where lists should be?

---

## OUTPUT FORMAT

One file: `qa/2026-06-01-ui-qa-pass.md`. Structure:

```markdown
# XVN UI QA pass — <DATE>

Target: <URL>  ·  Auth: <method>  ·  Build SHA: <if visible>

## Summary

- Total findings: <n>
- P0 (blocker): <n>
- P1 (major): <n>
- P2 (minor): <n>
- P3 (polish): <n>
- Routes with zero findings: <list>
- Routes that failed to load at all: <list>

## Convention violations (cross-cutting)

[All popup violations, all right-sidebar-on-chat-rail violations, all
banned-terminology hits, listed once here with route references so a
fix-agent can batch them.]

## Findings by route

### /<route path>

**Loaded:** yes/no  ·  **Console errors:** <count>  ·  **Failed
requests:** <count>  ·  **Screenshot:** `qa/screenshots/<route>.png`

#### F-001 · P1 · Functional · <one-line title>

- **What's wrong:** <1–3 sentences, concrete.>
- **Reproduce:** <numbered steps from a fresh load.>
- **Expected:** <what should happen.>
- **Evidence:** console line / failed URL / screenshot path.
- **Suggested fix:** <if obvious, otherwise omit.>
- **Files likely involved:** `<route file path>`, `<component path>`
  (do a quick grep; don't guess).

#### U-014 · P2 · Usability · Information hierarchy · <title>

[same fields]

[Continue for every finding on this route, functional first then
usability, ordered by severity.]

### /<next route>

…

## Missing surfaces

[Features documented in `MANUAL.md`, `docs/operator/`, or
`team/contracts/` that have no UI surface. Each as its own finding.]

## Routes not reached

[Anything you couldn't get to — auth-gated, requires data you don't
have, etc. — with the reason.]
```

### Finding ID scheme

- `F-###` = functional, `U-###` = usability, `M-###` = missing surface,
  `C-###` = convention violation. Number sequentially within sweep.

### Severity rubric

- **P0** — route doesn't load, data corrupted, money/state at risk,
  security boundary broken.
- **P1** — major surface unusable, primary CTA dead, key data wrong or
  missing, convention violation that breaks layout (e.g. right sidebar
  squeezing chart).
- **P2** — secondary action broken, confusing copy, inconsistent
  component, minor convention violation (popup for a confirmation),
  empty/error state missing.
- **P3** — polish: spacing, icon choice, tooltip wording, hover state
  not quite right.

If a finding could be P1 or P2, **default to P1.** Triage agents can
downgrade; they rarely upgrade.

---

## RULES FOR THE DISPATCHER

1. Read `frontend/web/src/routes.tsx` first. Reconcile against the
   route table above; flag any route added since this prompt was
   written and assign it to a sub-agent.
2. Spawn one sub-agent per route family (10 total). Run families
   1–8 in parallel for Sweep 1. Families 9 (charts) and 10
   (chart-lab) can run in a second wave or be deprioritized if budget
   is tight — charts is production, chart-lab is devtool.
3. Sub-agents emit their findings into `qa/inbox/<family>-<sweep>.md`.
   You merge.
4. After Sweep 1, **re-run** the same route families for Sweep 2 with
   the usability brief. Don't merge sweeps inside a sub-agent — run
   them as separate dispatches so a sub-agent's context stays focused.
5. After both sweeps, merge into the single
   `qa/2026-06-01-ui-qa-pass.md`. Delete `qa/inbox/` after merge.
6. Save all screenshots to `qa/screenshots/<route-slug>-<finding-id>.png`.
   Reference them by relative path in findings.

## RULES FOR SUB-AGENTS

1. Stay in your assigned family. If you notice something egregious on
   a neighboring route while passing through, log it and move on —
   don't expand scope.
2. Capture evidence before writing a finding. No screenshot, no
   network trace, no console capture → no finding.
3. Reproduce every finding twice before logging it. Flaky behavior
   gets its own severity (P1) and is noted as "intermittent."
4. Write findings as you go, not at the end. Crash recovery should
   not cost more than one route's findings.
5. Don't suggest fixes unless they're obvious from the symptom. The
   prompt for the fix-agent is a separate problem.

## ANTI-PATTERNS — DO NOT

- Do **not** fix anything. Even a one-line CSS tweak. This is a
  detection pass.
- Do **not** open issues anywhere outside the report file. No GitHub
  issues, no triage tickets — those come later from the report.
- Do **not** infer findings from code reads. Every finding must be
  grounded in something you observed in the running browser.
- Do **not** collapse multiple distinct findings into one entry "for
  brevity." One issue = one finding. Fix-agents grab them
  independently.
- Do **not** be polite. "This feels a bit off" is useless. Say what's
  wrong, where, and how to reproduce.

---

## STARTUP CHECKLIST (dispatcher runs once)

- [ ] Confirm `<BASE_URL>` is reachable and the build SHA is captured.
- [ ] Confirm auth works — log in once, verify session persists.
- [ ] Create `qa/inbox/` and `qa/screenshots/`.
- [ ] Read `frontend/web/src/routes.tsx`, diff against the route
      family table.
- [ ] Read `CLAUDE.md` § Terminology and § Frontend UI rules.
- [ ] Skim `MANUAL.md` so missing-surface findings are grounded.
- [ ] Dispatch Sweep 1 to all 10 family sub-agents.
- [ ] Wait, merge to `qa/inbox/sweep1-merged.md`.
- [ ] Dispatch Sweep 2 to all 10 family sub-agents.
- [ ] Wait, merge to `qa/inbox/sweep2-merged.md`.
- [ ] Produce final `qa/2026-06-01-ui-qa-pass.md`, delete
      `qa/inbox/`.

Done.
