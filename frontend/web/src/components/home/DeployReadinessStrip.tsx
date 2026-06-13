// frontend/web/src/components/home/DeployReadinessStrip.tsx
//
// Deploy-readiness checklist strip (xvision-e17). A slim safety-gate band the
// route mounts directly under SafetyPauseBanner, above AttentionBand: it is a
// gate ("can I deploy?"), not a nag. When every check passes the strip
// collapses to a single calm "Ready to deploy" line; otherwise it lists the
// failing/unknown checks as tone-dot rows, each with a routed fix link.
//
// The component is presentational — it takes the already-built ReadinessCheck[]
// (see features/home/deploy-readiness.ts). The route owns fetching and calls
// buildDeployReadiness(); the integration agent mounts this with the result.
//
// HONESTY MANDATE: nothing here renders a money / P&L / capital figure. The
// detail strings come straight from the pure selector, which omits them.
//
// NO POPUP: this is an inline, full-width strip — no dialog/sheet/overlay.

import { Link } from "react-router-dom";

import { Card } from "@/components/primitives/Card";
import {
  isDeployReady,
  type ReadinessCheck,
  type ReadinessStatus,
} from "@/features/home/deploy-readiness";

export interface DeployReadinessStripProps {
  checks: ReadinessCheck[];
}

// Per-status leading glyph + color. Glyphs (not just color) carry the meaning
// so the row is legible to colour-blind operators and in monochrome.
const STATUS_GLYPH: Record<ReadinessStatus, string> = {
  pass: "✓",
  fail: "✗",
  unknown: "—",
};

// Saturated token colours; all three meet ≥4.5:1 on the card surface in both
// themes. `text-text-4` is the muted-but-readable tone for the unknown glyph.
const STATUS_GLYPH_CLASS: Record<ReadinessStatus, string> = {
  pass: "text-gold",
  fail: "text-danger",
  unknown: "text-text-4",
};

function CheckRow({ check }: { check: ReadinessCheck }) {
  return (
    <div
      data-testid={`readiness-row-${check.id}`}
      data-status={check.status}
      className="flex items-start gap-2 py-1.5"
    >
      <span
        className={`mt-px shrink-0 font-mono text-[12px] leading-5 ${STATUS_GLYPH_CLASS[check.status]}`}
        aria-hidden="true"
      >
        {STATUS_GLYPH[check.status]}
      </span>

      <div className="min-w-0 flex-1 text-[12px] leading-5">
        <span className="font-medium text-text-2">{check.label}</span>
        {check.detail && (
          <span className="ml-1 text-text-3">— {check.detail}</span>
        )}
        {check.link && (
          <>
            {" "}
            <Link
              to={check.link.to}
              className="text-text-3 underline underline-offset-2 transition-colors hover:text-text"
            >
              {check.link.label}
            </Link>
          </>
        )}
      </div>
    </div>
  );
}

export function DeployReadinessStrip({ checks }: DeployReadinessStripProps) {
  // Nothing fetched yet → render nothing (the band shrinks instead of stacking
  // an empty placeholder), matching the home page's "say nothing when you have
  // nothing to say" contract.
  if (checks.length === 0) return null;

  const ready = isDeployReady(checks);

  return (
    <section
      data-testid="deploy-readiness-strip"
      aria-label="Deploy readiness"
    >
      <Card className="px-5 py-2.5">
        {ready ? (
          // Collapsed one-line green state.
          <div className="flex items-center gap-2 text-[12px] leading-5">
            <span className="shrink-0 font-mono text-gold" aria-hidden="true">
              ✓
            </span>
            <span className="font-medium text-text-2">Ready to deploy</span>
          </div>
        ) : (
          <div className="divide-y divide-border-soft/60">
            {checks.map((check) => (
              <CheckRow key={check.id} check={check} />
            ))}
          </div>
        )}
      </Card>
    </section>
  );
}
