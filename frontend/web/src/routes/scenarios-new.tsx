import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { createScenario, scenarioKeys } from "@/api/scenarios";
import { ScenarioForm, type ScenarioFormDraft } from "@/components/scenario/ScenarioForm";
import { Topbar } from "@/components/shell/Topbar";
import { WizardPreviewChartV2Container } from "@/components/chart/v2/surfaces/WizardPreviewChartV2Container";

const DEFAULT_DRAFT: ScenarioFormDraft = {
  from: "",
  to: "",
  granularity: "1h",
};

export function ScenariosNewRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [error, setError] = useState<string | undefined>(undefined);
  const [draft, setDraft] = useState<ScenarioFormDraft>(DEFAULT_DRAFT);

  const m = useMutation({
    mutationFn: createScenario,
    onSuccess: (s) => {
      qc.invalidateQueries({ queryKey: scenarioKeys.all });
      navigate(`/scenarios/${s.id}`);
    },
    onError: (err) => {
      setError(err instanceof ApiError ? err.message : String(err));
    },
  });

  return (
    <>
      <Topbar title="New scenario" sub="" />
      <div className="px-6 py-5 max-w-3xl space-y-4">
        <ScenarioForm
          submitting={m.isPending}
          error={error}
          onDraftChange={setDraft}
          onSubmit={(req) => {
            setError(undefined);
            m.mutate(req);
          }}
          onCancel={() => navigate("/scenarios")}
        />
        <WizardPreviewChartV2Container
          asset="ETH"
          from={draft.from}
          to={draft.to}
          granularity={draft.granularity}
          includeBaseline
        />
      </div>
    </>
  );
}
