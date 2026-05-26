// src/features/marketplace/components/AgentIcon.tsx
// Bot glyph used wherever an agent (🤖) count appears — no emoji, brand control.
export function AgentIcon({ size = 11, className = "" }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 12 12"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
      aria-hidden="true"
    >
      <rect x="2" y="3" width="8" height="6.5" rx="1.5" />
      <circle cx="4.5" cy="6.2" r="0.6" fill="currentColor" />
      <circle cx="7.5" cy="6.2" r="0.6" fill="currentColor" />
      <path d="M6 1.5v1.5" />
      <circle cx="6" cy="1.1" r="0.4" fill="currentColor" />
    </svg>
  );
}
