/**
 * GrainOverlay — full-bleed pin-stripe noise texture behind the
 * B4 GradientHeroDashboard content. Pure CSS repeating-linear-gradient
 * at opacity 0.5; subtle by design (it sits under the AuraBackground
 * + content layer).
 */
import type { ReactElement } from "react";

export function GrainOverlay(): ReactElement {
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0"
      style={{
        zIndex: 0,
        opacity: 0.5,
        background:
          "repeating-linear-gradient(0deg, rgba(241,236,221,0.012) 0 1px, transparent 1px 3px)",
      }}
      data-testid="grain-overlay"
    />
  );
}
