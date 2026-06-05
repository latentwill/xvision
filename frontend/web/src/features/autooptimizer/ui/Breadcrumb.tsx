import { Link } from "react-router-dom";

export type Crumb = { label: string; to?: string };

export function Breadcrumb({ items }: { items: Crumb[] }) {
  return (
    <nav aria-label="Breadcrumb" className="mb-4 flex items-center gap-2 font-mono text-[11px] text-text-3">
      {items.map((c, i) => {
        const last = i === items.length - 1;
        return (
          <span key={`${c.label}-${i}`} className="flex items-center gap-2">
            {c.to && !last ? (
              <Link to={c.to} className="uppercase tracking-wide hover:text-text">
                {c.label}
              </Link>
            ) : (
              <span aria-current={last ? "page" : undefined} className={last ? "text-text" : "uppercase tracking-wide"}>{c.label}</span>
            )}
            {!last ? <span aria-hidden className="text-text-4">›</span> : null}
          </span>
        );
      })}
    </nav>
  );
}
