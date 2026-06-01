// AutoOptimizerLayout — top-level container for /autooptimizer/*.
//
// Five tabs:
//   Live       → LiveCycleView   (SSE event feed)
//   Genealogy  → GenealogyTree   (lineage nodes)
//   Diff       → DiffInspector   (mutation node detail; active when :hash present)
//   Ladder     → ExperimentWriterLadder (mutator scoreboard)
//   Provenance → LadderWithProvenance  (ladder + lineage context)
//
// Tab state is driven by the URL:
//   /autooptimizer           → Live tab
//   /autooptimizer/diff/:hash → Diff tab (hash forwarded to DiffInspector)
//   ?tab=genealogy|diff|ladder|provenance → other tabs via query param
//
// No popups — all content is inline per CLAUDE.md rules.

import { useEffect, useState } from "react";
import { useSearchParams, useLocation, useParams } from "react-router-dom";

import { Topbar } from "@/components/shell/Topbar";
import { LiveCycleView } from "./LiveCycleView";
import { GenealogyTree } from "./GenealogyTree";
import { DiffInspector } from "./DiffInspector";
import { ExperimentWriterLadder } from "./ExperimentWriterLadder";
import { LadderWithProvenance } from "./LadderWithProvenance";

type Tab = "live" | "genealogy" | "diff" | "ladder" | "provenance";

const TABS: [Tab, string][] = [
  ["live", "Live"],
  ["genealogy", "Genealogy"],
  ["diff", "Diff"],
  ["ladder", "Ladder"],
  ["provenance", "Provenance"],
];

function tabFromSearch(param: string | null, pathHasDiff: boolean): Tab {
  if (pathHasDiff) return "diff";
  switch (param) {
    case "genealogy":
      return "genealogy";
    case "diff":
      return "diff";
    case "ladder":
      return "ladder";
    case "provenance":
      return "provenance";
    default:
      return "live";
  }
}

export function AutoOptimizerLayout() {
  const [searchParams, setSearchParams] = useSearchParams();
  const location = useLocation();
  // :hash is only present on nested /autooptimizer/diff/:hash routes
  const { hash } = useParams<{ hash?: string }>();

  const pathHasDiff = location.pathname.includes("/diff/") && !!hash;

  const [tab, setTab] = useState<Tab>(() =>
    tabFromSearch(searchParams.get("tab"), pathHasDiff),
  );

  // Re-sync when the URL changes (back/forward, deep-link).
  useEffect(() => {
    setTab(tabFromSearch(searchParams.get("tab"), pathHasDiff));
  }, [searchParams, pathHasDiff]);

  const handleTabChange = (t: Tab) => {
    setTab(t);
    if (t === "live") {
      setSearchParams({}, { replace: true });
    } else {
      setSearchParams({ tab: t }, { replace: true });
    }
  };

  return (
    <>
      <Topbar
        title="Optimizer"
        sub="Live experiments, genealogy, and experiment-writer performance"
      />

      <div className="space-y-5">
        {/* Tab bar */}
        <div
          role="tablist"
          aria-label="Optimizer views"
          className="flex gap-4 border-b border-border"
        >
          {TABS.map(([t, label]) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              onClick={() => handleTabChange(t)}
              className={`pb-2 -mb-px border-b-2 text-[13px] font-medium transition-colors ${
                tab === t
                  ? "border-gold text-text"
                  : "border-transparent text-text-3 hover:text-text-2"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Tab content */}
        {tab === "live" && <LiveCycleView />}
        {tab === "genealogy" && <GenealogyTree />}
        {tab === "diff" && <DiffInspector />}
        {tab === "ladder" && <ExperimentWriterLadder />}
        {tab === "provenance" && <LadderWithProvenance />}
      </div>
    </>
  );
}
