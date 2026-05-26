// src/features/marketplace/components/VerifiedBadge.tsx
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
