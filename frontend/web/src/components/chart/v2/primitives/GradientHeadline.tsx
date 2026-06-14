/**
 * GradientHeadline — B4's hero topbar headline. Renders the prefix +
 * suffix in Geist; the bracketed phrase gets a linear-gradient text
 * fill (`90deg, #5EEAD4 → #00E676 → #00B85F`). An optional emphasised
 * number renders in Geist Mono gold beside it.
 *
 * The text-fill technique uses `background-clip: text` + transparent
 * color; `-webkit-background-clip` for Safari compatibility.
 */
import type { ReactElement, ReactNode } from "react";

export interface GradientHeadlineProps {
  prefix?: ReactNode;
  bracketed: ReactNode;
  suffix?: ReactNode;
  /** Optional number string (e.g. "+82.41%"). */
  emphasis?: string;
}

export function GradientHeadline({
  prefix,
  bracketed,
  suffix,
  emphasis,
}: GradientHeadlineProps): ReactElement {
  return (
    <h1
      className="text-[30px] leading-[1.1] tracking-normal text-text font-medium"
      style={{ fontFamily: 'Geist, sans-serif' }}
    >
      {prefix}
      {prefix ? " " : ""}
      <em
        className="not-italic"
        style={{
          background:
            "linear-gradient(90deg, #5EEAD4 0%, #00E676 35%, #00B85F 80%)",
          WebkitBackgroundClip: "text",
          backgroundClip: "text",
          color: "transparent",
          fontWeight: 600,
        }}
      >
        {bracketed}
      </em>
      {suffix ? " " : ""}
      {suffix}
      {emphasis && (
        <>
          {" "}
          <span
            className="inline-block ml-2 text-[26px] text-gold tabular-nums"
            style={{ fontFamily: 'Geist Mono, ui-monospace, monospace' }}
          >
            {emphasis}
          </span>
        </>
      )}
    </h1>
  );
}
