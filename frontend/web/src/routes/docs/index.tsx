import { useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
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
 * Supports `?slug=<slug>` query param for deep-linking: the rail, wizard,
 * inspector, or any other component can navigate to a specific docs page
 * by appending `?slug=<slug>` to `/docs`. Clicking a sidebar item updates
 * the URL so the page is reload-safe and shareable. Browser back/forward
 * navigates between pages.
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

  const [searchParams, setSearchParams] = useSearchParams();
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

  // Group the filtered pages by `section`, preserving the order in which
  // each section first appears. The backend guarantees pages with the same
  // section are contiguous, so a linear walk is enough — no re-sorting.
  const grouped = useMemo(() => {
    const groups: { section: string; pages: typeof filtered }[] = [];
    for (const p of filtered) {
      const last = groups[groups.length - 1];
      if (last && last.section === p.section) {
        last.pages.push(p);
      } else {
        groups.push({ section: p.section, pages: [p] });
      }
    }
    return groups;
  }, [filtered]);

  // Derive the active slug from the URL param. If the param names a slug
  // that isn't in the loaded index (unknown or index not yet loaded), fall
  // back to the first entry so the main pane always shows something useful.
  const urlSlug = searchParams.get("slug");
  const slugInIndex = urlSlug != null && pages.some((p) => p.slug === urlSlug);
  const activeSlug = slugInIndex
    ? urlSlug
    : (pages[0]?.slug ?? null);

  const page = useQuery({
    queryKey: activeSlug ? docsKeys.page(activeSlug) : docsKeys.all,
    queryFn: () => getDocsPage(activeSlug!),
    enabled: !!activeSlug,
    staleTime: 60_000,
  });

  return (
    <>
      <Topbar title="Docs" sub="In-app reference" />
      {/*
        Two-pane reader on tablet/desktop, stacked single-column on
        phone. Sidebar uses the folio-dark prototype's treatment (see
        `docs/design/xvnwiki/docs/docs.css`): hairline right border,
        sticky inside the scroll container, muted uppercase section
        eyebrows (10.5px / 0.14em tracking), and a left-edge accent on
        the active item (`inset 2px 0 0 var(--accent)`). Main pane caps
        the reader measure at ~880px so long lines stay scannable per
        the prototype's `.main { max-width: 880px }` block.
      */}
      <div className="md:grid md:grid-cols-[240px_1fr] md:gap-8 flex flex-col gap-4">
        <aside
          className="md:border-r md:border-border-soft md:pr-6 md:sticky md:top-4 md:self-start md:max-h-[calc(100vh-48px)] md:overflow-y-auto"
          aria-label="Docs navigation"
        >
          <input
            type="search"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter docs…"
            aria-label="Filter docs"
            className="w-full bg-surface-elev border border-border-soft rounded-sm px-2.5 py-1.5 text-[13px] text-text placeholder:text-text-3 focus:outline-none focus:border-gold/40 mb-3"
          />
          <nav className="flex flex-col gap-0.5" data-testid="docs-index">
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
              grouped.map((group, gi) => (
                <div
                  key={group.section}
                  className={`flex flex-col gap-0.5 ${gi > 0 ? "mt-5" : ""}`}
                  data-testid={`docs-section-${group.section.toLowerCase().replace(/\s+/g, "-")}`}
                >
                  <h3
                    className="text-[10.5px] uppercase text-text-3 font-semibold tracking-[0.14em] px-2 pb-1.5"
                    data-testid="docs-section-header"
                  >
                    {group.section}
                  </h3>
                  {group.pages.map((p) => {
                    const isActive = p.slug === activeSlug;
                    return (
                      <button
                        key={p.slug}
                        type="button"
                        onClick={() => setSearchParams({ slug: p.slug })}
                        aria-current={isActive ? "page" : undefined}
                        data-testid={`docs-index-item-${p.slug}`}
                        className={`text-left text-[13px] rounded-sm px-2 py-[5px] transition-colors leading-snug ${
                          isActive
                            ? "text-text bg-gold/10 shadow-[inset_2px_0_0_theme(colors.gold.DEFAULT)]"
                            : "text-text-2 hover:text-text hover:bg-surface-elev"
                        }`}
                      >
                        {p.title}
                      </button>
                    );
                  })}
                </div>
              ))
            )}
          </nav>
        </aside>

        <main className="min-w-0 md:max-w-[880px]">
          <Card className="p-6 md:p-10">
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
