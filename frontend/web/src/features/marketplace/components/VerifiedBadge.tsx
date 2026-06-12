// src/features/marketplace/components/VerifiedBadge.tsx
//
// Catalogue wax-seal glyph (overhaul §5, signature moment #2). A small
// antique-gilt seal — a filled gilt circle with an embossed check — that
// reads as "attested on-chain" rather than a loud green badge.
//
// NOTE on wording: for API (on-chain) listings, `verification === "verified"`
// is derived from `attestation_count > 0` — and v1 attestations are
// permissionless SELF-attestations (anyone, including the seller, can post
// one). The seal title therefore says "Attested on-chain"; the
// attestation-specific UI (LineageRoute "Eval attestations" section) already
// says "attested". Same export + props so every caller (browse + detail)
// keeps working.
export function VerifiedBadge({ "data-testid": testId }: { "data-testid"?: string } = {}) {
  return (
    <span
      data-testid={testId}
      title="Attested on-chain"
      aria-label="Attested on-chain"
      className="group/seal inline-flex h-[18px] w-[18px] items-center justify-center rounded-full bg-gilt-bg ring-1 ring-gilt/40 text-gilt transition-transform duration-200 motion-safe:hover:rotate-[8deg]"
    >
      <svg
        width="11"
        height="11"
        viewBox="0 0 12 12"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        aria-hidden="true"
        className="drop-shadow-[0_0.5px_0_rgba(0,0,0,0.35)]"
      >
        <path d="M2.5 6.5l2.2 2.2L9.5 3.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </span>
  );
}
