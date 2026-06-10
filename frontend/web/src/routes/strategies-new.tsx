import { useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import { createStrategy, type CreateStrategyOut } from "@/api/strategies";

export function StrategiesNewRoute() {
  const navigate = useNavigate();
  const [name, setName] = useState("");

  const create = useMutation<CreateStrategyOut, unknown, string>({
    mutationFn: (displayName) =>
      createStrategy({
        name: displayName.trim() || "Untitled strategy",
        creator: null,
      }),
    onSuccess: (out) => {
      navigate(`/strategies/${encodeURIComponent(out.id)}`, { replace: true });
    },
  });

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (create.isPending) return;
    create.mutate(name);
  }

  return (
    <>
      <Topbar title="New strategy" />

      <Card className="p-5 max-w-lg">
        <form onSubmit={onSubmit} className="space-y-4">
          <div>
            <h2 className="m-0 font-sans font-medium text-[20px] tracking-tight">
              Create strategy
            </h2>
            <p className="m-0 mt-1 text-text-3 text-[12px]">
              Give your strategy a name to get started. You can change it later.
            </p>
          </div>

          <div>
            <label
              htmlFor="strategy-name"
              className="block text-[12px] text-text-2 mb-1"
            >
              Name
            </label>
            <input
              id="strategy-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Untitled strategy"
              autoFocus
              disabled={create.isPending}
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] focus:outline-none focus:border-text-3 disabled:opacity-50"
            />
          </div>

          {create.isError ? (
            <div role="alert" className="space-y-1">
              <div className="text-[13px] text-rose-300 font-sans font-semibold">
                couldn't create strategy
              </div>
              <code className="text-rose-300/80 font-mono text-[12px]">
                {errorDetail(create.error)}
              </code>
            </div>
          ) : null}

          <div className="flex items-center justify-between gap-2 pt-1">
            <Link
              to="/strategies"
              className="text-[13px] text-text-3 hover:text-text"
            >
              Cancel
            </Link>
            <button
              type="submit"
              disabled={create.isPending}
              className="px-3.5 py-1.5 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {create.isPending ? "Creating…" : "Create strategy"}
            </button>
          </div>
        </form>
      </Card>
    </>
  );
}

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
