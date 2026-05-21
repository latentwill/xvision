import { useEffect, useRef, useState } from "react";
import type { TocItem } from "./extractToc";

// Right-rail "On this page" TOC with scrollspy. Mirrors the prototype's
// `.toc` block in `docs/design/xvnwiki/docs/docs.css` — hairline left
// border on the link list, a gold active rule, level-2 headings flush
// and level-3 indented. The active heading is whichever H2/H3 has just
// scrolled past a ~96px top offset.

const OFFSET = 96;

function findActive(items: TocItem[]): string | null {
  let current: string | null = null;
  for (const item of items) {
    const el = document.getElementById(item.id);
    if (!el) continue;
    const r = el.getBoundingClientRect();
    if (r.top - OFFSET <= 0) current = item.id;
    else break;
  }
  // Edge case: when scrolled to the bottom, pin the last heading.
  const atBottom =
    window.scrollY + window.innerHeight >=
    document.documentElement.scrollHeight - 4;
  if (atBottom && items.length) current = items[items.length - 1].id;
  return current;
}

export function DocsToc({ items }: { items: TocItem[] }) {
  const [activeId, setActiveId] = useState<string | null>(null);
  const rafRef = useRef<number | null>(null);

  useEffect(() => {
    if (items.length === 0) {
      setActiveId(null);
      return;
    }
    function recompute() {
      rafRef.current = null;
      setActiveId(findActive(items));
    }
    function schedule() {
      if (rafRef.current != null) return;
      rafRef.current = window.requestAnimationFrame(recompute);
    }
    recompute();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    return () => {
      if (rafRef.current != null) {
        window.cancelAnimationFrame(rafRef.current);
      }
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
    };
  }, [items]);

  if (items.length === 0) return null;

  return (
    <aside
      className="hidden lg:block sticky top-4 self-start max-h-[calc(100vh-48px)] overflow-y-auto pl-4 pr-2 text-[12px]"
      aria-label="On this page"
      data-testid="docs-toc"
    >
      <h5 className="text-[10.5px] uppercase text-text-3 font-semibold tracking-[0.14em] mb-2.5">
        On this page
      </h5>
      <ul className="list-none m-0 p-0 border-l border-border-soft">
        {items.map((item) => {
          const isActive = item.id === activeId;
          return (
            <li key={item.id} className={item.level === 3 ? "pl-2" : ""}>
              <a
                href={`#${item.id}`}
                onClick={(e) => {
                  // Smooth-scroll without losing the hash (router-friendly).
                  const el = document.getElementById(item.id);
                  if (!el) return;
                  e.preventDefault();
                  el.scrollIntoView({ behavior: "smooth", block: "start" });
                  history.replaceState(null, "", `#${item.id}`);
                }}
                className={[
                  "block py-1 leading-snug -ml-px border-l transition-colors",
                  item.level === 3
                    ? "pl-[22px] text-[11.5px]"
                    : "pl-3",
                  isActive
                    ? "text-gold border-gold"
                    : item.level === 3
                      ? "text-text-4 border-transparent hover:text-text-3"
                      : "text-text-3 border-transparent hover:text-text-2",
                ].join(" ")}
                data-testid={`docs-toc-link-${item.id}`}
                data-active={isActive ? "true" : undefined}
              >
                {item.text}
              </a>
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
