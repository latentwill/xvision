# Product

## Register

product

## Users

xvision serves a single technical operator who monitors, authors, backtests, and deploys AI-assisted trading strategies against their own broker account. Users are comfortable with dense dashboards, strategy code, risk limits, eval runs, and operational controls. They need the interface to expose consequences clearly because strategy defects or risk-engine misconfiguration can lose real money.

## Product Purpose

xvision is a non-custodial AI trading-agent workbench. It lets operators author strategy bundles, evaluate them, compare runs, configure providers and brokers, and supervise live or paper execution while preserving explicit scope enforcement. Success means the operator can understand current system state, inspect strategy quality, and intervene before unsafe behavior reaches capital.

## Brand Personality

Calm technical control: precise, operational, safety-first, and terminal-adjacent. The UI should feel like a trustworthy instrument panel for expert work, not a marketing site or consumer autopilot.

## Anti-references

- Generic SaaS dashboard: no purple-gradient card grids, KPI theater, template admin chrome, or decorative metric blocks.
- Crypto casino terminal: no hype/trader-bro neon chaos, gamified risk, blinking overload, or visual language that encourages reckless action.
- Black-box consumer fintech: avoid hiding risk, scope, or execution state behind friendly abstractions.

## Design Principles

1. Make risk visible before action: controls that can affect capital must expose scope, limits, and failure modes.
2. Prefer operational clarity over decoration: density is acceptable when labels, hierarchy, and state are legible.
3. Keep controls consistent across surfaces: the same action class should use the same shape, focus behavior, menu treatment, and disabled state.
4. Preserve expert agency: searchable strategy controls, transparent filters, and inspectable choices matter more than simplified flows.
5. Be calm under volatility: live state can update, but motion and color should signal meaning rather than create urgency.

## Accessibility & Inclusion

Target WCAG AA. All interactive controls need accessible names, keyboard support, visible focus states, sufficient contrast, and reduced-motion-safe transitions. Dropdowns and comboboxes must preserve screen-reader state (`aria-expanded`, `aria-controls`, selected option state) and support keyboard operation without traps.
