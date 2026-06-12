// src/features/marketplace/routes/ShareComposer.tsx
// Share composer — collapsed by default to a ~56px strip.
// Expanding ("Customize post") reveals the OG preview + caption + post targets.
// No Discord (no real web intent). Chain notification hint as a single-line chip.
// No modals. In-flow inline-expand only.
import { useState } from "react";
import { ShareableCard } from "@/features/marketplace/components/ShareableCard";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import type { ShareComposerData } from "@/features/marketplace/data/types";

function buildTwitterUrl(caption: string, url: string): string {
  const params = new URLSearchParams({ text: caption, url });
  return `https://twitter.com/intent/tweet?${params.toString()}`;
}

function buildWarpcastUrl(caption: string, url: string): string {
  const params = new URLSearchParams({ text: `${caption} ${url}` });
  return `https://warpcast.com/~/compose?${params.toString()}`;
}

export function ShareComposer({
  share,
  initialExpanded = false,
}: {
  share: ShareComposerData;
  initialExpanded?: boolean;
}) {
  const { ogCard, buyerStamp, variants, notificationHint } = share;
  const [caption, setCaption] = useState(share.caption);
  const [expanded, setExpanded] = useState(initialExpanded);

  const shareUrl = `https://${ogCard.url}`;
  const twitterHref  = buildTwitterUrl(caption, shareUrl);
  const warpcastHref = buildWarpcastUrl(caption, shareUrl);

  return (
    <div className="p-3 flex flex-col gap-3">
      {/* ── Primary CTA row (always visible) ────────────────────────────────── */}
      <div className="flex items-center gap-2 flex-wrap">
        <a
          href={twitterHref}
          target="_blank"
          rel="noreferrer"
          className="bg-gold text-black font-mono text-[12.5px] font-semibold rounded py-2 px-4 flex items-center justify-center hover:opacity-90 motion-safe:active:scale-[0.96]"
        >
          Post to X
        </a>
        <a
          href={warpcastHref}
          target="_blank"
          rel="noreferrer"
          className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-3 py-2 flex items-center gap-1.5 hover:text-text"
        >
          Farcaster
          <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
            <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </a>
        <button
          onClick={() => {
            try {
              navigator.clipboard?.writeText(`${caption} ${shareUrl}`);
            } catch {
              // clipboard not available in this context (tests, insecure context)
            }
          }}
          className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-3 py-2 flex items-center gap-1.5 hover:text-text"
        >
          Copy link
          <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
            <rect x="4" y="4" width="7" height="7" rx="1" />
            <path d="M2 8V2h6" strokeLinecap="round" />
          </svg>
        </button>
        <button
          onClick={() => setExpanded((v) => !v)}
          className="font-mono text-[11px] text-text-3 border border-border-strong rounded px-2 py-1.5 hover:text-text ml-auto"
        >
          {expanded ? "Collapse" : "Customize post"}
        </button>
      </div>

      {/* ── Expanded composer (OG preview + caption + variants) ──────────────── */}
      {expanded && (
        <>
          {/* ── Mini OG preview ───────────────────────────────────────────────── */}
          <div>
            <div
              data-og-preview
              className="relative rounded-md border border-border overflow-hidden bg-black"
              style={{ aspectRatio: "1200 / 630" }}
            >
              {/* ShareableCard renders at 1200×630; CSS transform scales it to fit */}
              <div
                style={{
                  width: "1200px",
                  height: "630px",
                  transformOrigin: "top left",
                  transform: "scale(0.293)",
                }}
              >
                <ShareableCard data={ogCard} />
              </div>

              {/* Buyer stamp overlay — absolute top-right over the preview */}
              {buyerStamp && (
                <div
                  className="absolute top-1.5 right-1.5 px-2 py-0.5 rounded-sm bg-black/75 backdrop-blur font-mono text-[9.5px] text-text-3"
                  aria-label="buyer stamp"
                >
                  {buyerStamp}
                </div>
              )}
            </div>

            <div className="mt-1.5 flex items-center justify-between">
              <span className="font-mono text-[9.5px] uppercase tracking-[0.16em] text-text-3">
                OG CARD · 1200 × 630
              </span>
              <span className="font-mono text-[9.5px] text-text-3">
                twitter · warpcast · og
              </span>
            </div>
          </div>

          {/* ── Caption editor ────────────────────────────────────────────────── */}
          <div>
            <div className="font-mono text-[9px] uppercase tracking-[0.18em] text-text-3 mb-1.5">
              CAPTION
            </div>
            <textarea
              value={caption}
              onChange={(e) => setCaption(e.target.value)}
              rows={4}
              className="w-full px-2.5 py-2 rounded border border-border-strong bg-surface-elev text-[12.5px] text-text leading-snug resize-none font-sans focus:outline-none focus:border-gold/50"
            />
          </div>

          {/* ── Suggested variants ────────────────────────────────────────────── */}
          {variants.length > 0 && (
            <div className="rounded-sm border border-dashed border-border-strong p-2.5">
              <div className="font-mono text-[8.5px] uppercase tracking-[0.18em] text-text-3 mb-2">
                SUGGESTED VARIANTS
              </div>
              <div className="flex flex-col gap-1">
                {variants.map((v) => (
                  <button
                    key={v}
                    onClick={() => setCaption(v)}
                    className="font-mono text-[10.5px] text-text-3 text-left hover:text-text py-0.5"
                  >
                    ↳ {v}
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* ── Post-to targets ───────────────────────────────────────────────── */}
          <div>
            <div className="font-mono text-[9px] uppercase tracking-[0.18em] text-text-3 mb-1.5">
              POST TO
            </div>
            <div className="flex flex-wrap gap-1.5">
              <a
                href={twitterHref}
                target="_blank"
                rel="noreferrer"
                className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-2 py-1.5 flex items-center gap-1.5 hover:text-text"
              >
                X / Twitter
                <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
                  <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </a>

              <a
                href={warpcastHref}
                target="_blank"
                rel="noreferrer"
                className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-2 py-1.5 flex items-center gap-1.5 hover:text-text"
              >
                Farcaster
                <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
                  <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </a>
            </div>
          </div>
        </>
      )}

      {/* ── Chain-native notification hint — single-line chip (always visible) ── */}
      {notificationHint && (
        <div className="flex items-center gap-2 px-3 py-2 rounded border border-gold-soft bg-gold/10">
          <AgentIcon size={11} />
          <span className="font-mono text-[11px] text-gold">{notificationHint}</span>
        </div>
      )}
    </div>
  );
}
