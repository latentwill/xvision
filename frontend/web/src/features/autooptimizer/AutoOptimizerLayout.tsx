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
    case "genealogy": return "genealogy";
    case "diff": return "diff";
    case "ladder": return "ladder";
    case "provenance": return "provenance";
    default: return "live";
  }
}

export function AutoOptimizerLayout() {
  const [searchParams, setSearchParams] = useSearchParams();
  const location = useLocation();
  const { hash } = useParams<{ hash?: string }>();
  const pathHasDiff = location.pathname.includes("/diff/") && !!hash;
  const [tab, setTab] = useState<Tab>(() =>
    tabFromSearch(searchParams.get("tab"), pathHasDiff),
  );

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
        {tab === "live" && <LiveCycleView onTabChange={(t) => handleTabChange(t as Tab)} />}
        {tab === "genealogy" && <GenealogyTree />}
        {tab === "diff" && <DiffInspector />}
        {tab === "ladder" && <ExperimentWriterLadder />}
        {tab === "provenance" && <LadderWithProvenance />}
      </div>
    </>
  );
}
