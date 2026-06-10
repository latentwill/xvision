---
name: design-audit
description: Use when asked for a UX/UI audit, design review of the running product, "audit the site", or before a demo/launch — boots the real site, screenshots every page (desktop + mobile), audits like a senior design lead on day one, prioritizes P0-P3, fixes safe small issues on the spot, and writes an actionable report to docs/design-audit/.
---

# Full-site design audit: boot → screenshot → audit → prioritize → fix

Judge whether a normal user can **understand** the product, **trust** it, and
**finish the core action** without reading docs — not whether the UI looks nice.
Every finding must name the dimension it hurts (understanding / trust /
conversion) and the specific fix.

## 1. Boot & access

- Prefer the live/staging deployment (real data beats empty local fixtures).
  Verify with `curl -s -o /dev/null -w "%{http_code}" <url>`.
- Check auth: find the login route in the SPA router, then probe the session
  endpoint directly (`curl -X POST .../api/auth/session -d '{}'`). Note any
  auth-theater (page claims a token is required but the API issues one freely)
  as a trust finding.
- If verifying code fixes: boot local dev with the API proxied to the live
  backend (for xvision: edit the `/api` proxy target in
  `frontend/web/vite.config.ts` temporarily — **revert before commit**).

## 2. Enumerate pages from the router, not the nav

Read the route table (xvision: `frontend/web/src/routes.tsx`). Nav menus hide
orphaned routes — orphans are exactly what an audit must find. Pull real entity
ids from the API (`/api/strategies`, `/api/eval/runs`, …) so detail pages show
real data, not 404s.

## 3. Screenshot sweep (agent-browser)

```bash
agent-browser open "$BASE/"
# Kill first-run tours/overlays BEFORE capturing (xvision:)
agent-browser eval "localStorage.setItem('xvn.onboarding.first-run-tour.completed','1')"
agent-browser set viewport 1440 900     # desktop pass
# per page: open → wait 3000 → screenshot → collect console errors
agent-browser open "$BASE$path"; agent-browser wait 3000
agent-browser screenshot "assets/desktop-$name.png"
agent-browser console; agent-browser errors     # log per page
agent-browser set viewport 390 844      # mobile pass over the core flows
```

**Gotchas learned the hard way:**
- The full-page flag is `--full` (not `--full-page`).
- `screenshot --full` may not paint below-fold list rows (captureBeyondViewport
  artifact). For long lists: `agent-browser set viewport 1440 3600` and take a
  *normal* screenshot instead. Before calling blank rows a product bug, verify
  against the live DOM (`agent-browser eval`, scroll, viewport screenshot).
- Each new agent-browser session loses localStorage — re-seed tour-dismissal
  keys after `close`.
- Capture `agent-browser console` per page; repeated render warnings (chart
  libs choking on empty data) are trust findings and often point at real bugs.

## 4. Audit dimensions (score each page 1–5)

First impressions · navigation · visual hierarchy · component consistency ·
loading/empty/error states · trust signals · conversion paths. For every empty
or contradictory state, **trace it to code/API root cause** — "the dashboard
shows — " is a symptom; "the list endpoint never populates the join key" is a
finding. Diff the strongest page against the weakest: the gap is the report's
spine.

## 5. Prioritize

- **P0** broken/self-contradicting surfaces (crashes, data that denies other
  data on the same screen)
- **P1** major friction on the core flow (outcomes hidden, anonymous entities,
  junk data created by navigation)
- **P2** trust polish (copy bugs, console noise, env badges, auth theater)
- **P3** minor copy/spacing
End the report with: top 5 conversion-killers + 5 quick wins fixable today.

## 6. Fix safe small stuff on the spot

In-scope: copy, spacing, button hierarchy, responsive stacking, null-guards,
one-line join/type fixes — each with a regression test that mirrors **wire
reality** (JSON.parse gives numbers even when ts-rs types say bigint; fixtures
using `12n` mask production crashes). Out of scope: payment / delete / publish
actions, layout recomposition, backend contract changes — recommendations only.
Follow repo rules: work in `.worktrees/<name>`, run the touched test files +
`tsc --noEmit`, PR it.

## 7. Report

`docs/design-audit/README.md`: executive summary (lead with the sharpest
contrast found), scorecard table, findings F1…Fn each tagged
P0–P3 + dimension + evidence screenshot + specific fix, top-5 conversion
list, 5 quick wins (mark ✅ the ones fixed), method appendix with capture
gotchas. Screenshots in `docs/design-audit/assets/` named
`{desktop|mobile}-{page}.png`, fixes verified as `*-after-fix.png`.
