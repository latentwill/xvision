// src/features/marketplace/components/VerifiedBadge.tsx
//
// NOTE on wording: for API (on-chain) listings, `verification === "verified"`
// is derived from `attestation_count > 0` — and v1 attestations are
// permissionless SELF-attestations (anyone, including the seller, can post
// one). The badge label therefore overstates the trust signal on those
// surfaces; the attestation-specific UI (LineageRoute "Eval attestations"
// section) already says "attested". Parametrize/rename this label when the
// registry grows third-party verification.
export function VerifiedBadge({ "data-testid": testId }: { "data-testid"?: string } = {}) {
  return (
    <span
      data-testid={testId}
      title="Backtested + live-paper data committed on chain"
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-gold/40 text-gold text-[10px] font-medium"
    >
      <svg width="10" height="10" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
        <path d="M2.5 6.5l2.2 2.2L9.5 3.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      Verified
    </span>
  );
}
