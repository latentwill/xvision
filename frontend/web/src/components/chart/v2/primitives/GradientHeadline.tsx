/**
 * GradientHeadline — B4's hero topbar headline. Renders the prefix +
 * suffix in plain Cormorant; the bracketed phrase gets a linear-
 * gradient text fill (`90deg, #E5B86A → #D4A547 → #C16A3A`). An
 * optional emphasised number renders in JetBrains Mono gold beside it.
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
      style={{ fontFamily: '"Cormorant Garamond", serif' }}
    >
      {prefix}
      {prefix ? " " : ""}
      <em
        className="not-italic"
        style={{
          background:
            "linear-gradient(90deg, #E5B86A 0%, #D4A547 35%, #C16A3A 80%)",
          WebkitBackgroundClip: "text",
          backgroundClip: "text",
          color: "transparent",
          fontStyle: "italic",
        }}
      >
        {bracketed}
      </em>
      {suffix ? " " : ""}
      {suffix}
      {emphasis && (
        <span
          className="inline-block ml-3 text-[26px] text-gold tabular-nums"
          style={{ fontFamily: '"JetBrains Mono", monospace' }}
        >
          {emphasis}
        </span>
      )}
    </h1>
  );
}
