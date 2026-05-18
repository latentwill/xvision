import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { DocsMarkdown } from "@/features/docs/DocsMarkdown";
import { docsKeys, getDocsIndex, getDocsPage } from "@/api/docs";

/**
 * `/docs` — in-app documentation surface.
 *
 * Two-pane layout: sidebar lists baked pages from `/api/docs/index`,
 * main pane renders the selected page's markdown body. Includes a
 * client-side fuzzy filter so the operator can find a page by title
 * substring without leaving the route (acceptance: "Search across docs
 * index works (client-side fuzzy match acceptable).").
 *
 * No network fetch beyond the dashboard's own `/api/docs/*` routes;
 * the content is baked into the deployed image.
 */
export function DocsRoute() {
  const index = useQuery({
    queryKey: docsKeys.index(),
    queryFn: getDocsIndex,
    staleTime: 60_000,
  });

  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  const pages = index.data ?? [];

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return pages;
    return pages.filter(
      (p) =>
        p.title.toLowerCase().includes(q) ||
        p.slug.toLowerCase().includes(q),
    );
  }, [pages, filter]);

  const activeSlug = selectedSlug ?? pages[0]?.slug ?? null;

  const page = useQuery({
    queryKey: activeSlug ? docsKeys.page(activeSlug) : docsKeys.all,
    queryFn: () => getDocsPage(activeSlug!),
    enabled: !!activeSlug,
    staleTime: 60_000,
  });

  return (
    <>
      <Topbar title="Docs" sub="In-app reference" />
      <div className="grid grid-cols-[240px_1fr] gap-4">
        <aside className="space-y-2" aria-label="Docs navigation">
          <input
            type="search"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter docs…"
            aria-label="Filter docs"
            className="w-full bg-surface-elev border border-border rounded px-2 py-1 text-[13px] text-text placeholder:text-text-3"
          />
          <nav className="flex flex-col gap-1" data-testid="docs-index">
            {index.isPending ? (
              <div className="text-[12px] text-text-3 py-2">Loading…</div>
            ) : index.isError ? (
              <div
                role="alert"
                data-testid="docs-index-error"
                className="rounded border border-danger/30 bg-danger/[0.06] px-2 py-1.5 text-[12px] text-danger"
              >
                Could not load docs index.
              </div>
            ) : filtered.length === 0 ? (
              <div className="text-[12px] text-text-3 py-2">
                No pages match "{filter}".
              </div>
            ) : (
              filtered.map((p) => {
                const isActive = p.slug === activeSlug;
                return (
                  <button
                    key={p.slug}
                    type="button"
                    onClick={() => setSelectedSlug(p.slug)}
                    aria-current={isActive ? "page" : undefined}
                    data-testid={`docs-index-item-${p.slug}`}
                    className={`text-left text-[13px] rounded px-2 py-1.5 border transition-colors ${
                      isActive
                        ? "border-gold/40 text-text bg-gold/5"
                        : "border-border-soft text-text-2 hover:text-text"
                    }`}
                  >
                    {p.title}
                  </button>
                );
              })
            )}
          </nav>
        </aside>

        <main className="min-w-0">
          <Card className="p-6">
            {!activeSlug ? (
              <div className="text-text-3 text-[13px]">
                Select a page from the index.
              </div>
            ) : page.isPending ? (
              <div className="text-text-3 text-[13px]">Loading page…</div>
            ) : page.isError ? (
              <div
                role="alert"
                data-testid="docs-page-error"
                className="text-danger text-[13px]"
              >
                Could not load page <code>{activeSlug}</code>.
              </div>
            ) : (
              <article data-testid="docs-page-body">
                <DocsMarkdown body={page.data ?? ""} />
              </article>
            )}
          </Card>
        </main>
      </div>
    </>
  );
}
