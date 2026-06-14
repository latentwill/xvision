import { useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { DocsMarkdown } from "@/features/docs/DocsMarkdown";
import { useDocsPrefs } from "@/features/docs/useDocsPrefs";
import { docsKeys, getDocsIndex, getDocsPage } from "@/api/docs";

/**
 * `/docs` — in-app documentation surface.
 *
 * Two-pane layout (sidebar / article), matching the folio-dark prototype
 * density while giving the article the full remaining width.
 *
 * `?slug=<slug>` deep-links to a specific page. Display preferences
 * (density, TOC visibility) persist in localStorage. `⌘K` / `Ctrl+K`
 * focuses the page filter.
 *
 * Per the no-popups rule, the "Display options" panel is an
 * inline-expand below the sidebar — never a floating tweaks drawer
 * like the prototype's HTML mock.
 */
export function DocsRoute() {
  const index = useQuery({
    queryKey: docsKeys.index(),
    queryFn: getDocsIndex,
    staleTime: 60_000,
  });

  const [searchParams, setSearchParams] = useSearchParams();
  const [filter, setFilter] = useState("");
  const [showOptions, setShowOptions] = useState<boolean>(() => {
    try {
      return localStorage.getItem("xvn.docs.showOptions") === "true";
    } catch {
      return false;
    }
  });
  const [copied, setCopied] = useState(false);
  const filterRef = useRef<HTMLInputElement>(null);
  const { prefs, setDensity } = useDocsPrefs();

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

  const urlSlug = searchParams.get("slug");
  const slugInIndex = urlSlug != null && pages.some((p) => p.slug === urlSlug);
  const activeSlug = slugInIndex ? urlSlug : urlSlug == null ? (pages[0]?.slug ?? null) : null;

  const page = useQuery({
    queryKey: activeSlug ? docsKeys.page(activeSlug) : docsKeys.all,
    queryFn: () => getDocsPage(activeSlug!),
    enabled: !!activeSlug,
    staleTime: 60_000,
  });

  // ⌘K / Ctrl+K focuses the page filter. Mirrors the prototype's
  // keyboard hint without yet implementing full-text search.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        const tag = (e.target as HTMLElement | null)?.tagName ?? "";
        if (tag === "INPUT" || tag === "TEXTAREA") return;
        e.preventDefault();
        filterRef.current?.focus();
        filterRef.current?.select();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function handleCopyMarkdown() {
    if (!page.data) return;
    try {
      await navigator.clipboard.writeText(page.data);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard API unavailable; silently no-op.
    }
  }

  const densityArticleClass =
    prefs.density === "comfortable"
      ? "leading-[1.65] text-[14.5px]"
      : "leading-[1.55] text-[13.5px]";

  return (
    <>
      <Topbar title="Docs" sub="In-app reference" />
      <div className="md:grid md:grid-cols-[240px_minmax(0,1fr)] md:gap-8 flex flex-col gap-4">
        <aside
          className="md:border-r md:border-border-soft md:pr-6 md:sticky md:top-4 md:self-start md:max-h-[calc(100vh-48px)] md:overflow-y-auto"
          aria-label="Docs navigation"
        >
          <div className="relative mb-3">
            <input
              ref={filterRef}
              type="search"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter docs…"
              aria-label="Filter docs"
              className="w-full bg-surface-elev border border-border-soft rounded-sm pl-2.5 pr-10 py-1.5 text-[13px] text-text placeholder:text-text-3 focus:outline-none focus:border-gold/40"
            />
            <span
              className="absolute right-2 top-1/2 -translate-y-1/2 font-mono text-[10px] text-text-3 border border-border-soft rounded-sm px-1 leading-none py-0.5 pointer-events-none select-none"
              aria-hidden="true"
            >
              {typeof navigator !== "undefined" && navigator.platform.toUpperCase().includes("MAC") ? "⌘K" : "Ctrl K"}
            </span>
          </div>
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
                      <a
                        key={p.slug}
                        href={`?slug=${p.slug}`}
                        onClick={(e) => {
                          e.preventDefault();
                          setSearchParams({ slug: p.slug });
                        }}
                        aria-current={isActive ? "page" : undefined}
                        data-testid={`docs-index-item-${p.slug}`}
                        className={`text-left text-[13px] rounded-sm px-2 py-[5px] border-l-2 transition-colors leading-snug block ${
                          isActive
                            ? "text-text bg-gold/10 border-gold"
                            : "text-text-2 border-transparent hover:text-text hover:bg-surface-elev"
                        }`}
                      >
                        {p.title}
                      </a>
                    );
                  })}
                </div>
              ))
            )}
          </nav>

          <div className="mt-6 pt-4 border-t border-border-soft">
            <button
              type="button"
              onClick={() => setShowOptions((v) => {
                const next = !v;
                try { localStorage.setItem("xvn.docs.showOptions", String(next)); } catch { /* ignore */ }
                return next;
              })}
              aria-expanded={showOptions}
              aria-controls="docs-display-options"
              data-testid="docs-display-options-toggle"
              className="w-full flex items-center justify-between text-[10.5px] uppercase tracking-[0.14em] font-semibold text-text-3 hover:text-text-2 px-2"
            >
              <span>Display options</span>
              <span aria-hidden="true">{showOptions ? "−" : "+"}</span>
            </button>
            {showOptions && (
              <div
                id="docs-display-options"
                className="mt-3 px-2 flex flex-col gap-3"
              >
                <DocsPrefRow
                  label="Density"
                  value={prefs.density}
                  options={[
                    { value: "compact", label: "Compact" },
                    { value: "comfortable", label: "Comfy" },
                  ]}
                  onChange={(v) => setDensity(v as "compact" | "comfortable")}
                  testId="docs-pref-density"
                />
              </div>
            )}
          </div>
        </aside>

        <main className="min-w-0 md:max-w-[1120px]">
          <div className="flex items-center justify-end mb-2">
            <button
              type="button"
              onClick={handleCopyMarkdown}
              disabled={!page.data}
              data-testid="docs-copy-md"
              title="Copy this page as Markdown — useful for pasting into an LLM"
              className="text-[11.5px] font-mono text-text-3 hover:text-text border border-border-soft rounded-sm px-2 py-1 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              {copied ? "Copied" : "Copy as Markdown"}
            </button>
          </div>
          <Card className="p-6 md:p-10">
            {urlSlug != null && !slugInIndex ? (
              <div
                role="alert"
                data-testid="docs-page-not-found"
                className="text-[13px] text-text-2"
              >
                <p className="mb-2 font-medium text-text">Page not found</p>
                <p className="text-text-3">
                  No documentation page with slug{" "}
                  <code className="font-mono text-danger">{urlSlug}</code>{" "}
                  exists. Select a page from the sidebar.
                </p>
              </div>
            ) : !activeSlug ? (
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
              <article
                data-testid="docs-page-body"
                className={densityArticleClass}
              >
                <DocsMarkdown body={page.data ?? ""} />
              </article>
            )}
          </Card>
        </main>
      </div>
    </>
  );
}

function DocsPrefRow({
  label,
  value,
  options,
  onChange,
  testId,
}: {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  testId: string;
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-[12px] text-text-2">{label}</span>
      <div
        className="inline-flex border border-border-soft rounded-sm overflow-hidden"
        role="radiogroup"
        aria-label={label}
        data-testid={testId}
      >
        {options.map((opt, i) => {
          const on = opt.value === value;
          return (
            <button
              key={opt.value}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => onChange(opt.value)}
              data-value={opt.value}
              className={[
                "px-2 py-1 text-[11.5px] font-mono transition-colors",
                i > 0 ? "border-l border-border-soft" : "",
                on
                  ? "bg-gold/10 text-gold"
                  : "text-text-3 hover:text-text-2",
              ].join(" ")}
            >
              {opt.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
