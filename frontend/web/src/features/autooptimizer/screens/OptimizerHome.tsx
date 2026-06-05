import { Topbar } from "@/components/shell/Topbar";
import { LiveCycleView } from "../LiveCycleView";
import { RecentCyclesTable } from "../panels/RecentCyclesTable";
import { ExperimentWritersPanel } from "../panels/ExperimentWritersPanel";

export function OptimizerHome() {
  return (
    <>
      <Topbar title="Optimizer" sub="Tonight's run, experiment writers, and recent cycles" />
      <div className="space-y-5">
        {/* In-flight cycle + live event feed (existing dashboard body). */}
        <LiveCycleView embedded />
        <ExperimentWritersPanel />
        <RecentCyclesTable />
      </div>
    </>
  );
}
