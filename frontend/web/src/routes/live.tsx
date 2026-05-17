import { useEffect } from "react";
import { useParams } from "react-router-dom";

import { LiveChart } from "@/components/chart/LiveChart";
import { Topbar } from "@/components/shell/Topbar";
import { useTraceDock } from "@/stores/trace-dock";

export function LiveRoute() {
  const { id = "" } = useParams();

  useEffect(() => {
    useTraceDock.getState().setActiveRun(id || null, id ? "live" : "post-hoc");
  }, [id]);
  // For v1, deployment_id == run_id. Replace when Plan 2c (live deployment
  // model) lands the deployment → run mapping.
  return (
    <>
      <Topbar title="Live cockpit" sub={id || "—"} />
      <div className="px-6 py-5">
        <LiveChart runId={id} />
      </div>
    </>
  );
}
