import type { ReactNode } from "react";

type Props = {
  children: ReactNode;
};

export function LeadCardChrome({ children }: Props) {
  return (
    <div className="relative rounded-card border border-gold/35 bg-[linear-gradient(180deg,rgba(212,165,71,0.05),var(--surface-card)_38%)]">
      <div className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-gold/70 to-transparent" />
      {children}
    </div>
  );
}
