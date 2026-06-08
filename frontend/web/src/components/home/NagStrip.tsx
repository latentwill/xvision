// frontend/web/src/components/home/NagStrip.tsx
//
// Muted config-nag strip — provider API keys and broker credentials only.
// Renders at the bottom of the home page, below all other content.
// Returns null when there are no nag items (clean state).

import { useState } from "react";
import { Link } from "react-router-dom";

// ─── types ───────────────────────────────────────────────────────────────────

export type AttentionItem = {
  tone: "warn" | "danger" | "info";
  title: string;
  detail: string;
  link?: { to: string; label: string };
};

// ─── helpers ─────────────────────────────────────────────────────────────────

const TONE_DOT: Record<AttentionItem["tone"], string> = {
  warn: "bg-amber-400",
  danger: "bg-red-500",
  info: "bg-blue-400",
};

// ─── sub-components ──────────────────────────────────────────────────────────

function NagItem({ item }: { item: AttentionItem }) {
  return (
    <div className="flex items-start gap-2 py-1.5">
      {/* tone dot */}
      <span
        data-tone={item.tone}
        className={`mt-1 h-1.5 w-1.5 flex-shrink-0 rounded-full ${TONE_DOT[item.tone]}`}
        aria-hidden="true"
      />

      <div className="min-w-0 flex-1">
        <span className="text-[12px] font-medium text-muted-foreground">
          {item.title}
        </span>
        {item.detail && (
          <span className="ml-1 text-[11px] text-muted-foreground/70 truncate">
            — {item.detail}
          </span>
        )}
        {item.link && (
          <>
            {" "}
            <Link
              to={item.link.to}
              className="text-[11px] text-muted-foreground/90 underline underline-offset-2 hover:text-foreground transition-colors"
            >
              {item.link.label}
            </Link>
          </>
        )}
      </div>
    </div>
  );
}

// ─── main component ──────────────────────────────────────────────────────────

const VISIBLE_COUNT = 3;

export function NagStrip({ items }: { items: AttentionItem[] }) {
  const [showAll, setShowAll] = useState(false);

  if (items.length === 0) return null;

  const overflow = items.length - VISIBLE_COUNT;
  const visible = showAll ? items : items.slice(0, VISIBLE_COUNT);

  return (
    <section
      data-testid="nag-strip"
      className="border-t border-border/50 pt-2"
    >
      <div className="divide-y divide-border/30">
        {visible.map((item, i) => (
          <NagItem key={i} item={item} />
        ))}
      </div>

      {!showAll && overflow > 0 && (
        <button
          type="button"
          onClick={() => setShowAll(true)}
          className="mt-1 text-[11px] text-muted-foreground/70 hover:text-muted-foreground transition-colors"
        >
          + {overflow} more
        </button>
      )}

      {showAll && overflow > 0 && (
        <button
          type="button"
          onClick={() => setShowAll(false)}
          className="mt-1 text-[11px] text-muted-foreground/70 hover:text-muted-foreground transition-colors"
        >
          show less
        </button>
      )}
    </section>
  );
}
