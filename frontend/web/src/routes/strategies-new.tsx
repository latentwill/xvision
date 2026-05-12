import { useState } from "react";
import { useNavigate, Link } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Icon } from "@/components/primitives/Icon";
import { ApiError } from "@/api/client";
import {
  createStrategy,
  listTemplates,
  strategyKeys,
  type CreateStrategyOut,
  type TemplateInfo,
} from "@/api/strategies";

export function StrategiesNewRoute() {
  const navigate = useNavigate();
  const [pendingTemplate, setPendingTemplate] = useState<string | null>(null);

  const templates = useQuery({
    queryKey: strategyKeys.templates(),
    queryFn: listTemplates,
    staleTime: 5 * 60 * 1000,
  });

  const create = useMutation<CreateStrategyOut, unknown, TemplateInfo>({
    mutationFn: (template) =>
      createStrategy({
        template: template.name,
        name: template.display_name,
        creator: null,
      }),
    onMutate: (template) => {
      setPendingTemplate(template.name);
    },
    onSuccess: (out) => {
      navigate(`/authoring/${encodeURIComponent(out.id)}`);
    },
    onError: () => {
      setPendingTemplate(null);
    },
  });

  return (
    <>
      <Topbar
        title="New strategy"
        sub="Pick a template to start drafting. Everything's editable in the inspector afterward."
      />

      <div className="mb-4">
        <Link
          to="/strategies"
          className="inline-flex items-center gap-1.5 text-[13px] text-text-3 hover:text-text"
        >
          <span className="inline-block rotate-180">
            <Icon name="chevR" size={12} />
          </span>{" "}
          Back to strategies
        </Link>
      </div>

      {templates.isPending ? (
        <Card className="p-6 animate-pulse">
          <div className="h-4 w-48 bg-surface-elev rounded mb-3" />
          <div className="h-4 w-72 bg-surface-elev rounded" />
        </Card>
      ) : templates.isError ? (
        <Card className="p-6">
          <div className="font-serif italic text-[20px] text-danger mb-2">
            couldn't load templates
          </div>
          <p className="m-0 text-text-2 text-[13px]">
            <code className="text-danger font-mono text-[12px]">
              {errorDetail(templates.error)}
            </code>
          </p>
        </Card>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {(templates.data ?? []).map((t) => (
            <TemplateCard
              key={t.name}
              template={t}
              pending={pendingTemplate === t.name && create.isPending}
              disabled={create.isPending}
              onPick={() => create.mutate(t)}
            />
          ))}
        </div>
      )}

      {create.isError ? (
        <Card className="p-4 mt-4 border-rose-500/40">
          <div className="text-[13px] text-rose-300 font-serif italic mb-1">
            couldn't create strategy
          </div>
          <code className="text-rose-300/80 font-mono text-[12px]">
            {errorDetail(create.error)}
          </code>
        </Card>
      ) : null}
    </>
  );
}

function TemplateCard({
  template,
  pending,
  disabled,
  onPick,
}: {
  template: TemplateInfo;
  pending: boolean;
  disabled: boolean;
  onPick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPick}
      disabled={disabled}
      className="text-left bg-surface border border-border rounded p-4 hover:border-text-3 transition-colors disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:border-border flex flex-col gap-2"
    >
      <div className="flex items-start justify-between gap-3">
        <h3 className="m-0 font-serif font-medium text-[18px] tracking-tight text-text">
          {template.display_name}
        </h3>
        {pending ? (
          <span className="text-[12px] text-gold">creating…</span>
        ) : (
          <span className="text-text-3 group-hover:text-text">
            <Icon name="chevR" size={14} />
          </span>
        )}
      </div>
      <p className="m-0 text-[13px] text-text-2 leading-snug">
        {template.plain_summary}
      </p>
      <code className="mt-auto pt-1 text-[11px] text-text-3 font-mono">
        {template.name}
      </code>
    </button>
  );
}

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
