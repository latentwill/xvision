import { useEffect, useState } from "react";
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
} from "@/api/strategies";

export function StrategiesNewRoute() {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);
  const [template, setTemplate] = useState("custom");

  const templates = useQuery({
    queryKey: strategyKeys.templates(),
    queryFn: listTemplates,
    staleTime: 5 * 60 * 1000,
  });

  const create = useMutation<CreateStrategyOut, unknown, void>({
    mutationFn: () =>
      createStrategy({
        template,
        name: name.trim(),
        creator: null,
      }),
    onSuccess: (out) => {
      navigate(`/authoring/${encodeURIComponent(out.id)}`);
    },
  });
  const canCreate = name.trim().length > 0 && !create.isPending;
  const templateOptions = (templates.data ?? []).filter(
    (t) => t.name !== "custom",
  );
  const selectedTemplate = templateOptions.find((t) => t.name === template);

  useEffect(() => {
    if (!selectedTemplate || nameEdited || name.trim().length > 0) return;
    setName(selectedTemplate.display_name);
  }, [name, nameEdited, selectedTemplate]);

  function onTemplateChange(nextTemplate: string) {
    setTemplate(nextTemplate);
  }
  const cliCommand = `xvn strategy create --template ${shellQuote(template)} --name ${shellQuote(
    name.trim() || "Funding Fade Agent",
  )} --json`;

  return (
    <>
      <Topbar
        title="New strategy"
        sub="Name the strategy first. Templates are optional starters, not the default workflow."
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

      <Card className="p-5 max-w-3xl">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (canCreate) create.mutate();
          }}
          className="space-y-4"
        >
          <div className="block">
            <label
              htmlFor="strategy-name"
              className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5"
            >
              Name
            </label>
            <input
              id="strategy-name"
              value={name}
              onChange={(e) => {
                setNameEdited(true);
                setName(e.target.value);
              }}
              autoFocus
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[14px] text-text focus:outline-none focus:border-gold/40"
              placeholder="Funding Fade Agent"
            />
          </div>

          <div className="block">
            <label
              htmlFor="strategy-template"
              className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5"
            >
              Template
            </label>
            <select
              id="strategy-template"
              value={template}
              onChange={(e) => onTemplateChange(e.target.value)}
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
            >
              <option value="custom">Open form (no template)</option>
              {templateOptions.map((t) => (
                <option key={t.name} value={t.name}>
                  {t.display_name}
                </option>
              ))}
            </select>
            <p className="m-0 mt-1 text-[12px] text-text-3 leading-snug">
              Start empty, or pick a template to fill the blank form with its
              suggested defaults.
            </p>
            {selectedTemplate ? (
              <p className="m-0 mt-1 text-[12px] text-text-2 leading-snug">
                {selectedTemplate.plain_summary}
              </p>
            ) : null}
          </div>

          {templates.isError ? (
            <p className="m-0 text-text-3 text-[12px]">
              Templates unavailable:{" "}
              <code className="text-danger font-mono">
                {errorDetail(templates.error)}
              </code>
            </p>
          ) : null}

          <div className="rounded-sm border border-border-soft bg-surface-panel px-3 py-3">
            <div className="mb-2 text-[11px] uppercase tracking-wide text-text-3">
              Strategy-agent checklist
            </div>
            <ul className="m-0 space-y-1.5 p-0 text-[12px] text-text-2">
              {[
                "Create or attach a reusable agent",
                "Pick a configured provider/model",
                "Add a system prompt and risk-capable role",
              ].map((item) => (
                <li key={item} className="flex items-start gap-2">
                  <span className="mt-[6px] h-1.5 w-1.5 rounded-full bg-text-3" />
                  <span>{item}</span>
                </li>
              ))}
            </ul>
            <p className="m-0 mt-2 text-[12px] text-text-3 leading-snug">
              A new strategy draft is not eval-ready until this checklist is
              complete.
            </p>
          </div>

          <div>
            <div className="mb-1 text-[11px] uppercase tracking-wide text-text-3">
              CLI
            </div>
            <code className="block rounded-sm border border-border bg-surface-panel px-3 py-2 text-[12px] text-text font-mono overflow-x-auto whitespace-pre">
              {cliCommand}
            </code>
          </div>

          <div className="flex items-center justify-end gap-2 pt-2">
            <button
              type="submit"
              disabled={!canCreate}
              className="px-4 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {create.isPending ? "Creating…" : "Create strategy"}
            </button>
          </div>
        </form>
      </Card>

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

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}

function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_./:-]+$/.test(value)) return value;
  return `'${value.replace(/'/g, "'\\''")}'`;
}
