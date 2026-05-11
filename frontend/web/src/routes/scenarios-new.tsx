import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { createScenario, scenarioKeys } from "@/api/scenarios";
import { ScenarioForm } from "@/components/scenario/ScenarioForm";
import { Topbar } from "@/components/shell/Topbar";

export function ScenariosNewRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [error, setError] = useState<string | undefined>(undefined);

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
      <div className="px-6 py-5 max-w-3xl">
        <ScenarioForm
          submitting={m.isPending}
          error={error}
          onSubmit={(req) => {
            setError(undefined);
            m.mutate(req);
          }}
          onCancel={() => navigate("/scenarios")}
        />
      </div>
    </>
  );
}
