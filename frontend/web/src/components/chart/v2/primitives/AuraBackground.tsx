/**
 * AuraBackground — three radial-blur washes positioned behind the
 * content layer of the B4 GradientHeroDashboard. Pure CSS — no canvas,
 * no JS. Light themes get muted-desaturated variants; dark themes use
 * the handoff's full-strength gold/ember/plum/amber palette.
 *
 * Layout assumption: parent has `position: relative` and an inner
 * content layer with `position: relative; z-index: 1`. AuraBackground
 * places itself absolutely at inset:0 with z-index:0.
 */
import type { ReactElement } from "react";

export interface AuraBackgroundProps {
  /** Hide the auras (e.g. on the light theme until per-theme tuning
   *  lands). Defaults to false. */
  disabled?: boolean;
}

export function AuraBackground({
  disabled = false,
}: AuraBackgroundProps): ReactElement | null {
  if (disabled) return null;
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0"
      style={{ zIndex: 0 }}
      data-testid="aura-background"
    >
      {/* 520×520 gold/ember wash, top-left */}
      <div
        className="absolute"
        style={{
          top: 0,
          left: 0,
          width: 520,
          height: 520,
          opacity: 0.45,
          filter: "blur(20px)",
          background:
            "radial-gradient(closest-side, rgba(0,230,118,0.55), rgba(0,184,95,0.20) 60%, transparent 100%)",
        }}
      />
      {/* 680×680 ember/plum wash, bottom-right */}
      <div
        className="absolute"
        style={{
          bottom: -100,
          right: -260,
          width: 680,
          height: 680,
          opacity: 0.40,
          filter: "blur(20px)",
          background:
            "radial-gradient(closest-side, rgba(0,184,95,0.45), rgba(94,234,212,0.18) 65%, transparent 100%)",
        }}
      />
      {/* 380×380 amber wash, top-right */}
      <div
        className="absolute"
        style={{
          top: 0,
          right: 0,
          width: 380,
          height: 380,
          opacity: 0.30,
          filter: "blur(20px)",
          background:
            "radial-gradient(closest-side, rgba(94,234,212,0.45), transparent 70%)",
        }}
      />
    </div>
  );
}
