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
  warn: "bg-warn",
  danger: "bg-danger",
  info: "bg-info",
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
        <span className="text-[12px] font-medium text-text-3">
          {item.title}
        </span>
        {item.detail && (
          <span className="ml-1 text-[11px] text-text-4 truncate">
            — {item.detail}
          </span>
        )}
        {item.link && (
          <>
            {" "}
            <Link
              to={item.link.to}
              className="text-[11px] text-text-3 underline underline-offset-2 hover:text-text transition-colors"
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
    <section data-testid="nag-strip" className="px-5 py-2">
      <div id="nag-strip-items" className="divide-y divide-border-soft/60">
        {visible.map((item, i) => (
          <NagItem key={i} item={item} />
        ))}
      </div>

      {!showAll && overflow > 0 && (
        <button
          type="button"
          aria-expanded={false}
          aria-controls="nag-strip-items"
          onClick={() => setShowAll(true)}
          className="mt-1 text-[11px] text-text-4 hover:text-text-3 transition-colors"
        >
          + {overflow} more
        </button>
      )}

      {showAll && overflow > 0 && (
        <button
          type="button"
          aria-expanded={true}
          aria-controls="nag-strip-items"
          onClick={() => setShowAll(false)}
          className="mt-1 text-[11px] text-text-4 hover:text-text-3 transition-colors"
        >
          show less
        </button>
      )}
    </section>
  );
}
