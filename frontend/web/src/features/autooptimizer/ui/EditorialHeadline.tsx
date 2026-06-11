import type { ReactNode } from "react";
import type { Headline } from "../selectors/buildHeadline";

/** Digest summary row. `tokens` is optional — omit the segment when absent. */
export type Digest = {
  experiments: number;
  kept: number;
  /** e.g. "31.8M" — omit to hide the tokens segment */
  tokens?: string;
  spend: string;
};

export function EditorialHeadline({
  headline,
  digest,
  children,
}: {
  headline: Headline;
  digest: Digest | null;
  children?: ReactNode;
}) {
  return (
    <div className="flex items-end justify-between gap-6 flex-wrap">
      <div className="min-w-0 max-w-[780px]">
        <h1 className="text-[24px] font-semibold tracking-tight leading-tight">
          {headline.title}{" "}
          <span className="text-text-3 font-normal">{headline.subtitle}</span>
        </h1>
        {digest && (
          <div className="mt-2.5 font-mono text-[11.5px] text-text-3 flex flex-wrap items-center gap-x-0">
            <span className="text-text-2">{digest.experiments} experiments</span>
            <span className="px-1.5 text-text-4">this week</span>
            <span className="mx-1.5 text-text-4">·</span>
            <span className="text-gold">{digest.kept} kept</span>
            {digest.tokens != null && (
              <>
                <span className="mx-1.5 text-text-4">·</span>
                <span className="text-text-2">{digest.tokens} tokens</span>
              </>
            )}
            <span className="mx-1.5 text-text-4">·</span>
            <span className="text-gold">{digest.spend} spend</span>
          </div>
        )}
      </div>
      {children && (
        <div className="flex items-center gap-2">{children}</div>
      )}
    </div>
  );
}
