// Command palette (⌘K). Mounted once in `Layout`. Press ⌘K (or Ctrl+K on
// Linux/Win) anywhere in the app to open; type to fuzzy-search across
// strategies, runs, scenarios, and a small set of named actions; Enter
// follows the selected row via react-router.
//
// Keyboard contract:
//   ⌘K / Ctrl+K     toggle
//   Esc             close (also via <dialog> backdrop)
//   ↑ / ↓           move selection
//   Enter           navigate to selected row
//
// One palette per app — the modal is mounted in Layout above <Outlet/>.

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useNavigate } from "react-router-dom";

import {
  searchArtifacts,
  type SearchHit,
  type SearchKind,
} from "@/api/search";
import { useUi } from "@/stores/ui";
import { isMacPlatform, modKeyLabel } from "@/lib/platform";
import { PRIMARY_NAV } from "@/components/shell/nav";

const DEBOUNCE_MS = 80;
const MAX_RESULTS = 40;

// Primary page entries come straight from the shared PRIMARY_NAV list (single
// source of truth with the Sidebar), so the palette can never drift out of sync
// again — every sidebar page (incl. Optimizer) is reachable from ⌘K.
const NAV_ACTIONS: SearchHit[] = PRIMARY_NAV.map((it) => ({
  kind: "action",
  artifact_id: `nav:${it.to}`,
  title: it.label,
  summary: it.summary,
  tags: ["nav"],
  href: it.to,
  updated_at: "",
  bm25_score: 0,
}));

// Deep-links that aren't top-level sidebar pages (create flow + settings
// sub-pages). Appended after the primary nav.
const EXTRA_ACTIONS: SearchHit[] = [
  { kind: "action", artifact_id: "nav:new-strategy", title: "New strategy", summary: "Create a blank strategy draft", tags: ["nav", "create"], href: "/strategies/new", updated_at: "", bm25_score: 0 },
  { kind: "action", artifact_id: "nav:settings-providers", title: "Settings · Providers", summary: "LLM keys & providers", tags: ["nav", "settings"], href: "/settings/providers", updated_at: "", bm25_score: 0 },
  { kind: "action", artifact_id: "nav:settings-brokers", title: "Settings · Brokers", summary: "Alpaca / Orderly credentials", tags: ["nav", "settings"], href: "/settings/brokers", updated_at: "", bm25_score: 0 },
  { kind: "action", artifact_id: "nav:settings-danger", title: "Settings · Danger zone", summary: "Reset / destructive ops", tags: ["nav", "settings"], href: "/settings/danger", updated_at: "", bm25_score: 0 },
];

// Static page-navigation entries. Always present; filtered client-side by the
// current query so "strategies" reliably resolves even before the backend
// index returns any artifact-level hits.
export const STATIC_ACTIONS: SearchHit[] = [...NAV_ACTIONS, ...EXTRA_ACTIONS];

function filterStatic(q: string): SearchHit[] {
  const needle = q.trim().toLowerCase();
  if (!needle) return STATIC_ACTIONS;
  return STATIC_ACTIONS.filter((a) => {
    const hay = `${a.title} ${a.summary} ${a.tags.join(" ")}`.toLowerCase();
    return hay.includes(needle);
  });
}

const KIND_ORDER: SearchKind[] = [
  "action",
  "strategy",
  "run",
  "finding",
  "scenario",
  "deployment",
  "journal_entry",
];

const KIND_LABEL: Record<SearchKind, string> = {
  action: "Actions",
  strategy: "Strategies",
  run: "Runs",
  finding: "Findings",
  scenario: "Scenarios",
  deployment: "Deployments",
  journal_entry: "Journal",
};

type Group = { kind: SearchKind; rows: SearchHit[] };

export function CommandPalette() {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const navigate = useNavigate();

  const open = useUi((s) => s.cmdkOpen);
  const setOpen = useUi((s) => s.setCmdkOpen);
  const toggle = useUi((s) => s.toggleCmdk);

  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [activeIdx, setActiveIdx] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // Bind ⌘K / Ctrl+K globally. Stable callback so the listener is registered
  // exactly once for the lifetime of the component.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const meta = isMacPlatform() ? e.metaKey : e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        toggle();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);

  // Sync open state with the <dialog> element. `showModal` traps focus +
  // renders the backdrop natively; `close` is fired by Esc and our own
  // close path. The `close` event covers both.
  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    if (open && !dlg.open) {
      dlg.showModal();
      setQuery("");
      setHits([]);
      setActiveIdx(0);
      setError(null);
      // Focus must happen after showModal; the input is keyboard-trapped
      // inside the dialog already.
      requestAnimationFrame(() => inputRef.current?.focus());
    } else if (!open && dlg.open) {
      dlg.close();
    }
  }, [open]);

  // The native <dialog> dispatches a `close` event when the user hits Esc
  // or invokes `dlg.close()`. Reflect that back into React state so the
  // ⌘K toggle stays in sync on the next press.
  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    function onClose() {
      setOpen(false);
    }
    dlg.addEventListener("close", onClose);
    return () => dlg.removeEventListener("close", onClose);
  }, []);

  // Debounced fetch. Fires on every keystroke; the previous timer is
  // cleared so only the last keystroke in a 80ms window hits the server.
  // If the backend search is unavailable, we silently fall back to the
  // static actions so navigation still works.
  useEffect(() => {
    if (!open) return;
    const controller = new AbortController();
    const timer = window.setTimeout(async () => {
      try {
        const result = await searchArtifacts({
          q: query,
          limit: MAX_RESULTS,
        });
        if (!controller.signal.aborted) {
          setHits(result);
          setActiveIdx(0);
          setError(null);
        }
      } catch (e) {
        if (!controller.signal.aborted) {
          // Static actions still render — degrade gracefully.
          const msg = e instanceof Error ? e.message : "search failed";
          setError(msg);
          setHits([]);
        }
      }
    }, DEBOUNCE_MS);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [query, open]);

  const groups: Group[] = useMemo(() => {
    // Always merge static actions with server hits so page nav resolves even
    // before the backend index is warm. Dedupe by artifact_id.
    const actions = filterStatic(query);
    const seen = new Set<string>();
    const merged: SearchHit[] = [];
    for (const a of actions) {
      if (!seen.has(a.artifact_id)) {
        seen.add(a.artifact_id);
        merged.push(a);
      }
    }
    for (const h of hits) {
      if (!seen.has(h.artifact_id)) {
        seen.add(h.artifact_id);
        merged.push(h);
      }
    }
    const buckets = new Map<SearchKind, SearchHit[]>();
    for (const h of merged) {
      const list = buckets.get(h.kind);
      if (list) list.push(h);
      else buckets.set(h.kind, [h]);
    }
    return KIND_ORDER.map((kind) => ({ kind, rows: buckets.get(kind) ?? [] }))
      .filter((g) => g.rows.length > 0);
  }, [hits, query]);

  // Flat order matches what arrow keys cycle through. Re-derived whenever
  // `groups` changes so activeIdx always points at a real row.
  const flatRows: SearchHit[] = useMemo(
    () => groups.flatMap((g) => g.rows),
    [groups],
  );

  useEffect(() => {
    setActiveIdx((i) => Math.min(i, Math.max(flatRows.length - 1, 0)));
  }, [flatRows.length]);

  const close = useCallback(() => setOpen(false), [setOpen]);

  const navigateTo = useCallback(
    (hit: SearchHit) => {
      close();
      // External hrefs (api endpoints) escape the SPA; everything else
      // routes via the SPA. v1 actions only ever produce SPA hrefs but
      // we keep this guard so external links still work if added later.
      if (hit.href.startsWith("/api/")) {
        window.location.href = hit.href;
      } else {
        navigate(hit.href);
      }
    },
    [navigate, close],
  );

  function onInputKey(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, Math.max(flatRows.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const hit = flatRows[activeIdx];
      if (hit) navigateTo(hit);
    }
  }

  // Backdrop click closes — clicks land on the <dialog> itself when they
  // hit the backdrop area outside the inner panel.
  function onDialogClick(e: React.MouseEvent<HTMLDialogElement>) {
    if (e.target === dialogRef.current) close();
  }

  function onQueryChange(nextQuery: string) {
    setQuery(nextQuery);
    setHits([]);
    setActiveIdx(0);
  }

  const listboxId = "cmd-palette-listbox";

  return (
    <dialog
      ref={dialogRef}
      role="dialog"
      onClick={onDialogClick}
      className="cmd-palette p-0 m-0 max-w-none w-full h-full bg-transparent backdrop:bg-black/60"
      aria-label="Command palette"
    >
      <div
        className="mx-auto mt-[14vh] w-[min(640px,90vw)] bg-surface-card border border-border rounded-card shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          type="text"
          role="combobox"
          aria-expanded={flatRows.length > 0}
          aria-haspopup="listbox"
          aria-autocomplete="list"
          aria-controls={listboxId}
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={onInputKey}
          placeholder="Jump to a strategy, run, scenario, or action…"
          autoComplete="off"
          spellCheck={false}
          className="w-full bg-transparent text-text px-4 py-3 text-[15px] border-0 border-b border-border-soft outline-none placeholder:text-text-3"
        />
        <div
          id={listboxId}
          role="listbox"
          aria-label="Search results"
          className="max-h-[60vh] overflow-y-auto"
        >
          {flatRows.length === 0 ? (
            <div className="px-4 py-3 text-text-3 text-xs">
              {query ? "No results." : "Start typing to search…"}
            </div>
          ) : (
            <PaletteResults
              groups={groups}
              flatRows={flatRows}
              activeIdx={activeIdx}
              onActivate={navigateTo}
              onHover={setActiveIdx}
            />
          )}
          {error ? (
            <div className="px-4 py-2 text-text-3 text-[11px] border-t border-border-soft">
              backend search degraded: {error}
            </div>
          ) : null}
        </div>
        <footer className="flex items-center gap-3 px-4 py-2 text-text-3 text-[11px] font-mono border-t border-border-soft">
          <span>{modKeyLabel()} K toggle</span>
          <span>↑↓ navigate</span>
          <span>↵ open</span>
          <span>esc close</span>
        </footer>
      </div>
    </dialog>
  );
}

function PaletteResults({
  groups,
  flatRows,
  activeIdx,
  onActivate,
  onHover,
}: {
  groups: Group[];
  flatRows: SearchHit[];
  activeIdx: number;
  onActivate: (hit: SearchHit) => void;
  onHover: (idx: number) => void;
}) {
  return (
    <ul className="m-0 p-0 list-none">
      {groups.map((group) => (
        <li key={group.kind}>
          <div className="px-4 pt-3 pb-1 text-[10px] uppercase tracking-wider text-text-3">
            {KIND_LABEL[group.kind]}
          </div>
          <ul className="m-0 p-0 list-none">
            {group.rows.map((hit) => {
              const flatIdx = flatRows.findIndex(
                (r) => r.kind === hit.kind && r.artifact_id === hit.artifact_id,
              );
              const active = flatIdx === activeIdx;
              return (
                <li key={`${hit.kind}:${hit.artifact_id}`}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={active}
                    onMouseEnter={() => onHover(flatIdx)}
                    onClick={() => onActivate(hit)}
                    className={`w-full text-left px-4 py-2 flex items-baseline gap-3 transition-colors ${
                      active
                        ? "bg-surface-hover text-text"
                        : "text-text-2 hover:bg-surface-hover"
                    }`}
                  >
                    <span className="flex-1 min-w-0">
                      <span className="block truncate text-text">
                        {hit.title}
                      </span>
                      {hit.summary ? (
                        <span className="block truncate text-text-3 text-xs">
                          {hit.summary}
                        </span>
                      ) : null}
                    </span>
                    <span className="text-text-3 text-[11px] font-mono shrink-0">
                      ↵
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </li>
      ))}
    </ul>
  );
}
