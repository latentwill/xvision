import { useEffect, useRef } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Icon } from "@/components/primitives/Icon";
import { ApiError } from "@/api/client";
import { createStrategy, type CreateStrategyOut } from "@/api/strategies";

export function StrategiesNewRoute() {
  const navigate = useNavigate();
  const started = useRef(false);
  const create = useMutation<CreateStrategyOut, unknown, void>({
    mutationFn: () =>
      createStrategy({
        name: "Untitled strategy",
        creator: null,
      }),
    onSuccess: (out) => {
      navigate(`/authoring/${encodeURIComponent(out.id)}`, { replace: true });
    },
  });

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    create.mutate();
  }, [create]);

  return (
    <>
      <Topbar title="New strategy" sub="Creating a blank draft..." />

      <Card className="p-5 max-w-3xl">
        {create.isError ? (
          <div role="alert">
            <div className="text-[13px] text-rose-300 font-sans font-semibold mb-1">
              couldn't create strategy
            </div>
            <code className="text-rose-300/80 font-mono text-[12px]">
              {errorDetail(create.error)}
            </code>
            <div className="mt-4">
              <Link
                to="/strategies"
                className="inline-flex items-center gap-1.5 text-[13px] text-text-3 hover:text-text"
              >
                <span className="inline-block rotate-180">
                  <Icon name="chevR" size={12} />
                </span>
                Back to strategies
              </Link>
            </div>
          </div>
        ) : (
          <div className="text-[13px] text-text-3">Creating strategy...</div>
        )}
      </Card>
    </>
  );
}

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
