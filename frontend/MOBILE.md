# xvision mobile UI/UX

Mobile design for the xvision web SPA at `frontend/web/`. This is an amendment
to [`DESIGN.md`](./DESIGN.md), which explicitly ruled mobile out of v1
(see DESIGN.md §10 "Phased delivery" and Appendix C). Mobile is now in scope
post-v1 as a layered surface on top of the same React app — not a separate
build, not a native shell.

**Author**: synthesis on 2026-05-11 from an ideonomy-rich brainstorm session.
See "Ideonomy trail" at the bottom for the operator moves and what wasn't
surfaced (worth a second pass).

---

## 1. Scope

**Mobile xvision is a responsive PWA layer on the existing Vite + React SPA.**
The same routes, the same engine API surface, the same `xvision-dashboard`
backend. The phone is a second viewport on the same app, not a separate
codebase.

**In scope**:
- All v1 desktop routes (`/`, `/setup`, `/strategies`, `/authoring/:id`,
  `/eval/runs`, `/eval/runs/:id`, `/eval/compare`, `/settings/*`).
- Mobile-first chat-and-agent flows for trading + evaluating.
- Touch-native charting (pinch, pan, tap-to-inspect).
- Inspector that survives a 390×844 viewport without losing capability.
- Remote access via Tailscale (see §6).

**Out of scope** (deferred — see §8):
- Native iOS / Android apps. No React Native, no Expo, no Capacitor.
- System-level integrations: lockscreen cards, CarPlay, Apple Watch
  complications, OS push notifications targeting a system tray.
- Camera-based chart recognition, voice composer.
- Multi-user / social mobile features.

---

## 2. Web-platform constraints (what we can't do)

The original brainstorm leaned on lockscreen verdict cards, native push,
ambient widgets. None of that is available to a web app. The honest list:

| Brainstorm idea | Web reality | Workable substitute |
|---|---|---|
| Lockscreen verdict card | Not possible | In-app card; Web Push if PWA-installed; CLI/agent notification |
| Native push to system tray | Limited (Web Push, no iOS Safari support pre-16.4; requires install) | Server-side delivery via agent CLI / desktop notification; in-app polling |
| Apple Watch complication | Not possible | Deferred; would need native app |
| CarPlay audio narration | Not possible | Deferred; would need native app |
| Home-screen widget | Not possible (web) | PWA shortcut + first-screen brief |
| Camera-based chart scan | Possible (getUserMedia) but heavy | Deferred to post-mobile-v1 |
| Background sync / waking up | Limited (Service Worker bg sync, flaky) | Server-driven push when installed; otherwise on-open refresh |

**Notification path forward**: the agent runtime (or a small CLI shim) is the
authoritative notifier. It surfaces decisions that need attention via whatever
channel the user has wired up — email, Slack, Telegram, a desktop daemon,
eventually Web Push for PWA-installed mobile users. The mobile web app
displays those notifications when it's open and polls a recent-activity feed;
it does not pretend to be a system-level notifier.

---

## 3. Three pillars

The mobile experience is anchored on three surfaces. Everything else degrades
gracefully or is reachable via deeper navigation.

### 3.1 Autorouter (chat-as-intent-shell)

The mobile home screen IS the chat. One composer, one stream, one agent.
Typing "evaluate my mean-reversion bundle on Q4 2024 with high-vol regime"
should:

1. Capture intent.
2. Draft the run spec (visible inline, editable).
3. Ask one clarifying question if ambiguous.
4. Launch the run.
5. Pin a progress card to the top of the stream.
6. Notify on completion (in-app + Web Push if installed).

Chat is not just text-Q&A. The agent dispatches to domain-specific actions:

| Intent surface | What the agent produces inline |
|---|---|
| "Run eval on X" | Run-spec card → confirm → run progress card |
| "Compare run A vs run B" | Two-run delta card with mini equity overlay |
| "Why did bundle Y veto AAPL?" | Decision trace card with cycle_id link |
| "Show me bundles for high-vol earnings" | Bundle carousel (swipe through results) |
| "Find a new strategy for chop regime" | Kicks autoresearcher; returns when candidate found |
| "Draft from this finding" | Opens setup wizard pre-seeded |
| "Approve" / "Veto" (in response to a pinned verdict card) | Commits decision |

**Why chat-first on mobile (and not desktop)**: on a 390×844 viewport,
navigation hierarchies cost more thumb-distance than they save in clarity.
Chat collapses route → screen → tab → detail into intent → action. Desktop
keeps the explicit IA because the canvas is bigger and parallel views are
cheap.

**Implementation**:
- The chat rail from DESIGN.md §7 becomes the *primary* surface on mobile,
  not a side panel. On phones <768px it occupies the whole viewport;
  the conventional routes are reachable via a slide-up drawer.
- Same backend (`POST /api/chat-rail/chat` SSE), same `ContextScope` enum,
  same session persistence. No mobile-specific backend.
- Inline action cards render the same component family used in run progress
  / verdict notifications elsewhere. One card library, three placements
  (chat inline, top-pinned, full-screen modal).

### 3.2 Charting (touch-native)

Charts are how the user understands what happened. On mobile they must:

- Render an equity curve, returns histogram, drawdown band, or trade-marker
  series in **≤1 frame on a mid-range phone** (target: 250ms TTI on a
  cold load with cached data).
- Support **pinch-to-zoom along time**, **pan**, **tap-a-trade-marker to
  open the cycle**, **long-press for a value tooltip**.
- Handle **two-series overlay** for compare (run A vs run B equity, with
  divergence shading) on a single canvas.
- Degrade to a sparkline at narrow widths (cards, list rows).

**Library decision**: the desktop plan in DESIGN.md §2 starts with hand-rolled
SVG and considers Recharts later. For mobile, hand-rolled SVG fails on touch
gestures and performance — too much DOM. **Pick a real chart library now.**

Candidates ranked for this use case:

| Library | Pro | Con |
|---|---|---|
| **Lightweight Charts (TradingView)** | Built for financial; canvas-based; touch + pinch; tiny (35KB) | Limited to TV's chart styles; harder to deeply customize |
| uPlot | Fastest; tiny | No built-in touch gestures; manual pinch/pan code |
| Visx | Composable; D3 under the hood | Bigger bundle; touch is DIY |
| Recharts | Familiar API | Slow on mobile with many points; SVG-based |

**Recommendation**: **Lightweight Charts** for the equity-curve / trade-marker
surface (the dominant case), kept hand-rolled (current) for tiny sparklines
in list rows. Two chart implementations, picked by component scale.

There's an active plan thread in `docs/superpowers/plans/` already
prototyping TradingView Lightweight Charts on the dashboard
(`2026-05-11-tradingview-*`). Mobile should ride that adoption, not fork it.

**Charting contract on mobile**:
- Tap on the equity curve: opens a vertical line + tooltip with timestamp,
  equity value, drawdown %, and the next decision card.
- Pinch on the time axis: zooms in/out smoothly; double-tap resets.
- Swipe-left on the chart (when zoomed): pages forward in time.
- A trade marker is a tap target ≥44px (Apple HIG); chart computes
  density-aware clustering when markers would overlap below that size.

### 3.3 Inspector (the 4-column survival problem)

The desktop Inspector (`/authoring/:id`) is the densest screen in xvision:
sidebar (200) · bundle outline (220) · split editor (flex) · validation rail
(280). All four columns are visible simultaneously on a 1440px display. On a
phone, they can't be.

**Mobile inspector structure**:

```
┌─────────────────────────────────┐
│  ← Bundle name        [⋯ menu]  │  <- top bar with back, name, overflow
├─────────────────────────────────┤
│  [Outline] [Editor] [Validate]  │  <- segmented control (3 tabs)
├─────────────────────────────────┤
│                                 │
│   active tab content            │
│   (full-width)                  │
│                                 │
├─────────────────────────────────┤
│  Talk to this bundle... [send]  │  <- persistent agent composer
└─────────────────────────────────┘
```

Three tabs, one persistent agent composer at the bottom that can act on the
currently-viewed slot ("explain this prompt", "what if I changed the
threshold to 0.6", "preview this slot on AAPL").

**Outline tab**: the bundle tree from desktop, condensed. Each layer is a
collapsible row; tapping a slot opens it in the Editor tab.

**Editor tab**: a single slot config form at a time. Live preview is a
collapsible section below the form, not a side-by-side split. Fixture picker
at the top of the preview section.

**Validate tab**: full warning list + token estimate + bundle JSON (expandable).

**Why one composer at the bottom is the right call**: the agent can read the
visible tab's state, so "explain this prompt" works whether you're in
Outline, Editor, or Validate. The composer is the constant; the canvas
changes underneath. This is the chat-as-intent-shell pattern from §3.1 applied
inside a deeper screen.

---

## 4. Notifications, async, and the agent

The phone is not a system-tray surface. It is a window into the agent's
running state, polled or pushed.

**Three delivery modes, in order of capability**:

1. **In-app foreground**: the app is open, the chat is visible, a card appears
   in the stream when something needs attention. Server-Sent Events on the
   chat-rail channel + a recent-activity poll fallback.
2. **Web Push (PWA-installed)**: user has installed the PWA via "Add to Home
   Screen". Notifications go through the browser's Web Push API. iOS supports
   this only from 16.4+ and only for installed PWAs. Use for: eval run done,
   risk-gate decision needs approval, autoresearcher found a candidate.
3. **CLI / agent-side notifier**: for everything that needs to reach the user
   when the app isn't installed or open, the agent emits to whatever channel
   is configured: email, Slack, Telegram, the desktop daemon's tray. The
   mobile web app doesn't try to be this channel — it consumes the same
   activity feed that the channels do.

**Authoritative source**: there is ONE activity-feed endpoint (probably an
extension of `/api/health` or a dedicated `/api/activity`). All three modes
read from it. The mobile app polls when foregrounded, subscribes via Web Push
when installed, and the CLI watches it server-side.

**Decision approval cards** (risk-gate verdicts, autoresearcher proposals):
- Foreground: pin to top of chat stream.
- Web Push: notification deep-links to the relevant card.
- CLI: emits a one-liner with a `xvn` command to approve/veto.

---

## 5. Per-screen mobile adaptations

Each desktop route gets a mobile treatment. The table below names the
adaptation pattern; details follow only where the pattern is unusual.

| Route | Desktop pattern | Mobile pattern |
|---|---|---|
| `/` Home | KPI grid + tables + chat rail | Brief card stack + chat composer |
| `/setup` | Two-column wizard | Single-column wizard (chat dominates) |
| `/strategies` | Wide table | Card list with swipe-actions |
| `/authoring/:id` | 4-column inspector | Three-tab inspector (§3.3) |
| `/eval/runs` | Wide table + filters bar | Card list + filter sheet |
| `/eval/runs/:id` | Multi-section detail page | Vertical scroll with sticky tabs |
| `/eval/compare` | Side-by-side columns | Stacked sections + overlay chart at top |
| `/settings/*` | Sub-route navigation | iOS-style settings list pattern |

### 5.1 Home — `/`

Desktop is a KPI grid. Mobile is a **morning brief** card stack above the
chat composer:

```
┌─────────────────────────────────┐
│  Good morning, latentwill       │
│  ─────────────────────────       │
│  📈  3 runs completed overnight │  <- one tappable summary card
│  ⚠  1 verdict awaiting approval │  <- tap → opens decision card
│  💡 Autoresearcher found 2      │
│      candidate bundles          │
│                                 │
│  [Equity sparkline, 7d]         │
│                                 │
│  ─── chat ───                   │
│  Yesterday: arm B won on...     │  <- chat stream below
│  ...                            │
│                                 │
│  [What do you want to do? ___]  │  <- composer at bottom
└─────────────────────────────────┘
```

The brief replaces the desktop's "Quick start cards" — those four CTAs become
chat suggestions ("Run a new eval", "Open last bundle", ...) the user can tap
to seed the composer.

### 5.2 Strategies — `/strategies`

Cards with:
- Bundle name + template badge
- Last eval Sharpe (one number, big)
- Tiny equity sparkline
- Status pill (Validated / Warnings / Draft)
- Swipe-left for quick actions: Run eval, Open inspector, Archive
- Tap to open inspector

Filter bar collapses to a single `[Filter]` button that opens a bottom sheet.

### 5.3 Eval runs — `/eval/runs`

Same card pattern. Each card shows: bundle, started_at, status, Sharpe,
Return, tiny equity sparkline. Long-press to enter multi-select mode for
Compare; selection count appears in a sticky bottom bar with a Compare button.

### 5.4 Run detail — `/eval/runs/:id`

Vertical scroll with section anchors that become a sticky horizontal scroll
tab bar once the user has scrolled past the header. Sections: Header KPIs,
Equity chart, Findings, Trade ledger.

The equity chart is the largest screen element — uses Lightweight Charts (§3.2).

### 5.5 Compare — `/eval/compare`

Top: overlay equity chart of both runs (same Lightweight Charts canvas, two
series, color-coded). Below: vertical stack of paired KPIs (Sharpe A | Sharpe
B with delta), then paired findings lists. Swipe-left on the overlay to flip
which series is "A".

Cap at 2 runs on mobile (desktop allows 3). Trying to navigate to a 3-run
compare from mobile shows "Mobile compare supports 2 runs; drop one."

### 5.6 Setup wizard — `/setup`

Already mostly chat-shaped on desktop. On mobile, drop the right-side
"Strategy in progress" panel into a top sticky summary strip that expands on
tap. The chat is the whole screen.

### 5.7 Settings

iOS-style grouped list: Providers / Brokers / Daemon / Identity / Danger
zone. Each opens a sub-screen. Form fields stack vertically with full-width
inputs, large tap targets.

---

## 6. Tailscale / remote access (operational)

The dashboard binds `127.0.0.1:8788` by default
(`crates/xvision-cli/src/commands/dashboard.rs`). That's loopback-only;
Tailscale traffic to the Mac's tailnet IP gets refused at the kernel.

**To reach the dashboard from a Tailscale-connected phone**:

```sh
# Bind to all interfaces (Tailscale ACLs gate who can reach the machine)
xvn dashboard serve --bind 0.0.0.0:8788

# Or narrower: bind only the tailnet IP
xvn dashboard serve --bind 100.x.x.x:8788
```

Then on the phone: `http://<mac-tailnet-host>:8788/` or
`http://100.x.x.x:8788/`.

**For Vite dev server** (`pnpm dev` during frontend iteration), `vite.config.ts`
needs:

```ts
server: {
  port: 5180,
  strictPort: true,
  host: true,                          // bind 0.0.0.0
  allowedHosts: true,                  // any hostname (Bonjour .local, Tailscale, LAN IP)
  proxy: { "/api": "http://127.0.0.1:8788" },
}
```

Without `host: true`, Vite binds to localhost only. With Vite 5's default
`allowedHosts` (loopback + private-IP only), the server rejects any hostname
it doesn't recognize with `Blocked request. This host ("…") is not allowed.`
— which is what iPhone Safari sees when reaching the Mac via Bonjour
(`Eds-MacBook-Pro.local:5180`) on a shared Wi-Fi or via a custom DNS name.
Setting `allowedHosts: true` disables that check so any hostname resolves;
the dashboard has no auth either way (DESIGN.md §8.4), so the real trust
boundary is the network (tailnet ACL or trusted LAN), not Vite's allowlist.
Don't run `pnpm dev` on an untrusted shared network.

Trust model: the dashboard has no auth (DESIGN.md §8.4). Tailscale's identity
layer is the authentication. Don't expose `0.0.0.0` to a non-Tailscale
network without adding auth first.

---

## 7. Stack additions for mobile

On top of the existing stack (DESIGN.md §2), mobile adds:

| Layer | Choice | Why |
|---|---|---|
| Responsive breakpoints | Tailwind defaults (`sm:` 640, `md:` 768, `lg:` 1024) | Already in Tailwind; no extra config |
| Mobile detection | `useMediaQuery('(max-width: 768px)')` hook | Pure CSS where possible; JS hook for layout swaps (e.g., chat-rail-as-primary) |
| Charts | **TradingView Lightweight Charts** | Touch gestures + perf, see §3.2 |
| Web Push | `web-push` (server) + Notification API (client) | Standard; only used when PWA-installed |
| PWA manifest + service worker | Vite PWA plugin (`vite-plugin-pwa`) | Install-to-home-screen, offline shell, Web Push |
| Touch gesture library | None — use native pointer events | Avoid Hammer.js / react-use-gesture overhead; the surface is small |
| Bottom sheet primitive | `vaul` (Radix-compatible) | Filter sheets, action sheets, settings sub-views |

**Bundle budget**: target ≤200KB gzipped initial load. Lightweight Charts is
~35KB; the PWA plugin adds ~5KB runtime; vaul is ~3KB. Headroom exists.

---

## 8. Deferred (post mobile-v1)

In order of likely re-prioritization:

1. **Camera-first** — chart-screenshot regime recognition, scan-to-import.
   Requires a vision model in the engine API; high engineering cost, high
   "wow" factor.
2. **Voice composer** — speech-to-text input on the chat composer. Trivial to
   add (Web Speech API) but the audio quality and noisiness of trading
   environments makes it less useful than typed input. Defer until requested.
3. **Live narration** (CarPlay-like audio stream) — requires a native shell.
4. **Apple Watch** — would need native app. Highest glance-first value, highest
   build cost.
5. **Multi-user / social mobile** — leaderboards, copy-trading, follows.
   Cross-cutting concern; pair with the marketplace plan when that becomes a
   priority.
6. **Tablet** — neither phone nor desktop. The responsive design will Just Work
   on tablet at 768–1024px (desktop layout collapses one column), but
   tablet-specific UX (Apple Pencil annotations on charts, split-view multi-run
   inspection) is deferred.
7. **Adversarial UX** — anti-affordances that slow the user down before a
   panic trade (confirmation friction proportional to recent volatility,
   etc.). Worth a dedicated brainstorm.

---

## 9. Phased delivery

**Phase 0 — Tailscale access (today)**
- Add `host: true` and `allowedHosts` to `vite.config.ts`.
- Document the `--bind 0.0.0.0:8788` recipe in this file and in
  `crates/xvision-dashboard/README.md`.
- Verify both dev and baked-binary surfaces load from phone.

**Phase 1 — responsive baseline (1 week)**
- Audit every route for sub-768px layout. Most need wrap-to-vertical, a few
  need real rework (Inspector, Compare).
- Add `useMediaQuery('(max-width: 768px)')` hook + a `<MobileShell>` /
  `<DesktopShell>` switch at the App root.
- Bottom-sheet primitive (vaul) + sticky-bottom composer pattern.

**Phase 2 — chat-as-primary mobile shell (1 week)**
- Mobile home becomes the chat (§3.1, §5.1).
- Action cards (run spec, run progress, verdict, bundle carousel, compare
  delta) renderable inline in the chat stream.
- Slide-up drawer for explicit-route navigation.

**Phase 3 — charting upgrade (1 week)**
- Adopt Lightweight Charts on `/eval/runs/:id` and `/eval/compare`.
- Touch gestures: pinch, pan, tap-marker, long-press tooltip.
- Sparklines stay hand-rolled SVG.

**Phase 4 — inspector adaptation (1 week)**
- Three-tab mobile inspector (§3.3) with persistent agent composer.
- Live-preview as collapsible section (depends on engine `preview-slot`
  endpoint per DESIGN.md §6.4).

**Phase 5 — PWA + Web Push (1 week)**
- vite-plugin-pwa setup, manifest, service worker, install prompt.
- Web Push subscription flow on the mobile shell.
- `/api/activity` endpoint (or extend existing) as the single notification
  source.
- Server-side push fanout (after agent decides "this matters").

**Cutover criterion**: Phase 1 + 2 + 3 covers 80% of the value (the phone
becomes a usable trading + evaluating surface). Phase 4 lights up authoring.
Phase 5 turns it into a notification target without lockscreen-tier hacks.

---

## Ideonomy trail

Brainstorm operators used to derive this design:

- **Negation** of "chat is the primary surface" → siblings (glance, gesture,
  voice, camera). Web-app constraints rule out lockscreen/voice/camera as
  shippable today, leaving chat + gesture, which produced the mobile shell
  pattern and the touch-charting requirement.
- **Combination** of {modalities} × {xvision actions} → eight composite
  features. Web-app subset of those: autorouter (chat × launch-run),
  morning brief (in-app × summary), bundle carousel (gesture × browse),
  discovery feed (in-app × autoresearcher), race view (gesture × compare).
- **Tree** organon with input-modality as the sub-typing relation. Surfaced
  the gap: chat-first is one modality of N, not the root.

Not surfaced and worth a follow-up: multi-user/social, offline-first,
adversarial-UX, billing/cost surface on mobile, tablet, watch. Dimension
prompts that would reach those: `sociality`, `temporality`,
`adversariality`.
