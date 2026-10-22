# XVN QA Pass 1 — Dashboard + Settings

> QA run against `https://xvnej.tail2bb69.ts.net/` on 2026-06-01.
>
> Scope covered: Dashboard / Control Tower, Strategies, Agents, Scenarios, Charts, Eval Runs, Marketplace, Docs, Settings → Providers, Settings → Brokers, Settings → Danger zone, plus the stale `/settings/danger-zone` alias.
>
> This document records observed findings from exploratory QA. It is not a product spec.

## Summary

### Confirmed issues
- **Medium** — `GET /settings/danger-zone` is a stale route alias; it redirects to the Dashboard (`/`) instead of the danger page.
- **Medium** — The dashboard home still shows a live provider outage: `provider_http_error` with OpenAI-compat `524` on `https://economy-veterans-shopping-thin.trycloudflare.com/v1/chat/completions`.
- **Low** — The first-run tour overlay appears on fresh browser state across pages, but it is dismissible and did not block QA after closing it.

### Things that checked out
- **Dashboard** renders the core home surface, recent runs, and chart snapshot without console errors.
- **Strategies**, **Agents**, **Scenarios**, **Charts**, **Eval Runs**, **Marketplace**, **Docs**, and the main **Settings** subpages all rendered normally.
- **`/settings/danger`** is the correct danger-zone route and shows the expected destructive-operation guardrails.

## Findings

### 1) `/settings/danger-zone` is stale and redirects to Home

**Severity:** Medium  
**Area:** Routing / navigation  
**Type:** Functional / route mismatch

**Observed behavior:**
- Visiting `https://xvnej.tail2bb69.ts.net/settings/danger-zone` lands on the Dashboard home page.
- The final URL is `/`, not a danger page.
- The correct danger page is `/settings/danger`.

**Expected behavior:**
- Old bookmarks or aliases should either render the danger page or redirect to `/settings/danger`.

**Why it matters:**
- Stale external links and old bookmarks do not preserve the intended operator path.
- This is a real route contract mismatch, not just a cosmetic alias.

---

### 2) Dashboard home still surfaces a provider outage

**Severity:** Medium  
**Area:** Dashboard / system status  
**Type:** Operational / live-state visibility

**Observed behavior:**
- The home page shows **Needs attention** with:
  - `provider_http_error`
  - OpenAI-compat API error `524`
  - URL: `https://economy-veterans-shopping-thin.trycloudflare.com/v1/chat/completions`
- The page also shows **Alpaca credentials not set** (`APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`).

**Expected behavior:**
- The dashboard should keep surfacing degraded live-state clearly, but the underlying provider outage should be investigated separately from the UI.

**Why it matters:**
- This is the main visible health warning on the home surface.
- It can block provider-backed flows until the upstream provider is restored or reconfigured.

---

### 3) First-run tour overlay appears on fresh sessions

**Severity:** Low  
**Area:** UX / onboarding  
**Type:** UX / friction

**Observed behavior:**
- A first-run tour overlay appears on fresh browser contexts across pages.
- It can intercept clicks until dismissed.
- It is dismissible and did not reappear inside the same warmed-up session.

**Expected behavior:**
- This is acceptable if the onboarding is intentional.

**Why it matters:**
- It adds friction to fresh QA runs and can briefly interfere with click-driven testing.

## Notes on evidence

### Routes visited
- `/`
- `/strategies`
- `/agents`
- `/scenarios`
- `/charts/compare`
- `/eval-runs`
- `/marketplace`
- `/docs`
- `/settings/providers`
- `/settings/brokers`
- `/settings/danger`
- `/settings/danger-zone` (stale alias check)

### Representative checks
- Dashboard body text and route links were read back from Playwright.
- No console errors or page errors were observed on the core routes visited in this pass.
- The stale alias was confirmed by checking the final URL after navigation.
