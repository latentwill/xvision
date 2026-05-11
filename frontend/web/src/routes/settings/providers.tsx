import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  addProvider,
  listProviders,
  removeProvider,
  settingsKeys,
} from "@/api/settings";
import type {
  AddProviderRequest,
  ProviderKindStr,
  ProviderRow,
} from "@/api/types.providers";

const KIND_OPTIONS: { value: ProviderKindStr; label: string; hint: string }[] = [
  {
    value: "anthropic",
    label: "Anthropic",
    hint: "First-party Claude API. Set `ANTHROPIC_API_KEY`.",
  },
  {
    value: "openai-compat",
    label: "OpenAI-compatible",
    hint:
      "Any /v1/chat/completions endpoint — OpenAI, Together, vLLM, Ollama, llama.cpp.",
  },
  {
    value: "local-candle",
    label: "Local (candle)",
    hint: "In-process candle model. No HTTP, no api_key_env.",
  },
];

export function SettingsProvidersRoute() {
  const qc = useQueryClient();
  const [adding, setAdding] = useState(false);
  const list = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });

  const remove = useMutation({
    mutationFn: (name: string) => removeProvider(name),
    onSuccess: () => qc.invalidateQueries({ queryKey: settingsKeys.providers() }),
  });

  if (list.isPending) {
    return (
      <Card className="p-6 animate-pulse">
        <div className="h-4 w-48 bg-surface-elev rounded mb-3" />
        <div className="h-4 w-72 bg-surface-elev rounded" />
      </Card>
    );
  }
  if (list.isError || !list.data) {
    return (
      <Card className="p-6">
        <div className="font-serif italic text-[20px] text-danger mb-2">
          couldn't load providers
        </div>
        <p className="m-0 mb-4 text-text-2 text-[13px]">
          <code className="text-danger font-mono text-[12px]">
            {errorMessage(list.error)}
          </code>
        </p>
        <button
          onClick={() => list.refetch()}
          className="px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
        >
          Retry
        </button>
      </Card>
    );
  }

  const rows = list.data.providers;

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="flex items-center justify-between mb-3">
          <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
            LLM providers
          </h3>
          {!adding ? (
            <button
              onClick={() => setAdding(true)}
              className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
            >
              + Add provider
            </button>
          ) : null}
        </div>
        <p className="m-0 mb-4 text-text-3 text-[12px] leading-snug">
          Workspace providers registered in <code className="font-mono">config/default.toml</code>.
          API key values are never returned — only env-var presence. Synthetic
          rows (auto-derived) and providers referenced by the <code className="font-mono">[intern]</code>
          {" "}default Intern slot are read-only.
        </p>

        {adding ? (
          <AddProviderForm
            onCancel={() => setAdding(false)}
            onAdded={() => {
              setAdding(false);
              qc.invalidateQueries({ queryKey: settingsKeys.providers() });
            }}
          />
        ) : null}

        {rows.length === 0 ? (
          <div className="text-text-2 text-[13px] py-6 text-center">
            no providers registered
          </div>
        ) : (
          <table className="w-full mt-2">
            <thead>
              <tr className="text-text-3 text-[11px] uppercase tracking-wider text-left">
                <th className="py-2 pr-3 font-normal">Name</th>
                <th className="py-2 pr-3 font-normal">Kind</th>
                <th className="py-2 pr-3 font-normal">Base URL</th>
                <th className="py-2 pr-3 font-normal">API key env</th>
                <th className="py-2 pr-3 font-normal text-right">Status</th>
                <th className="py-2 pr-0 font-normal text-right" />
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => (
                <ProviderRowView
                  key={p.name}
                  row={p}
                  onRemove={() => remove.mutate(p.name)}
                  removeError={
                    remove.variables === p.name && remove.isError
                      ? errorMessage(remove.error)
                      : null
                  }
                  removeBusy={
                    remove.variables === p.name && remove.isPending
                  }
                />
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}

function ProviderRowView({
  row,
  onRemove,
  removeError,
  removeBusy,
}: {
  row: ProviderRow;
  onRemove: () => void;
  removeError: string | null;
  removeBusy: boolean;
}) {
  const locked = row.synthetic || row.referenced_by_intern;
  const lockReason = row.synthetic
    ? "synthetic (auto-derived) — read-only"
    : row.referenced_by_intern
      ? "backs the [intern] default Intern slot — edit [intern] first"
      : null;
  return (
    <>
      <tr className="border-t border-border-soft align-middle">
        <td className="py-2 pr-3">
          <div className="flex items-center gap-2">
            <code className="font-mono text-[13px] text-text">{row.name}</code>
            {row.synthetic ? <Pill>synthetic</Pill> : null}
            {row.referenced_by_intern ? <Pill tone="gold">intern</Pill> : null}
          </div>
        </td>
        <td className="py-2 pr-3 text-text-2 text-[12px] font-mono">
          {row.kind}
        </td>
        <td className="py-2 pr-3">
          <code className="font-mono text-[12px] text-text-2">
            {row.base_url}
          </code>
        </td>
        <td className="py-2 pr-3">
          {row.api_key_env ? (
            <code className="font-mono text-[12px] text-text-2">
              {row.api_key_env}
            </code>
          ) : (
            <span className="text-text-3 text-[12px]">(none)</span>
          )}
        </td>
        <td className="py-2 pr-3 text-right">
          {!row.api_key_env ? (
            <span className="text-text-3 text-[12px]">n/a</span>
          ) : row.api_key_set ? (
            <span className="text-gold text-[12px]">● set</span>
          ) : (
            <span className="text-warn text-[12px]">○ unset</span>
          )}
        </td>
        <td className="py-2 pr-0 text-right">
          <button
            onClick={onRemove}
            disabled={locked || removeBusy}
            title={lockReason ?? undefined}
            className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-danger hover:border-danger disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:text-text-2 disabled:hover:border-border"
          >
            {removeBusy ? "Removing…" : "Remove"}
          </button>
        </td>
      </tr>
      {removeError ? (
        <tr className="border-t border-border-soft/40">
          <td colSpan={6} className="py-2 pr-0 text-[12px] text-danger">
            {removeError}
          </td>
        </tr>
      ) : null}
    </>
  );
}

function AddProviderForm({
  onCancel,
  onAdded,
}: {
  onCancel: () => void;
  onAdded: () => void;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<ProviderKindStr>("openai-compat");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKeyEnv, setApiKeyEnv] = useState("");

  const add = useMutation({
    mutationFn: (req: AddProviderRequest) => addProvider(req),
    onSuccess: () => {
      onAdded();
    },
  });

  const kindMeta = KIND_OPTIONS.find((k) => k.value === kind)!;
  const submittable = name.trim() !== "" && baseUrl.trim() !== "";

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (!submittable) return;
        add.mutate({
          name: name.trim(),
          kind,
          base_url: baseUrl.trim(),
          api_key_env: apiKeyEnv.trim(),
        });
      }}
      className="border border-border-soft rounded-md p-4 mb-4 bg-surface-elev/30 space-y-3"
    >
      <div className="flex items-center justify-between">
        <span className="text-[13px] text-text">New provider</span>
        <button
          type="button"
          onClick={onCancel}
          className="text-[12px] text-text-3 hover:text-text"
        >
          Cancel
        </button>
      </div>
      <Field label="Name" hint="lowercase, digits, hyphens. e.g. openai, ollama-local.">
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. openai"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
      </Field>
      <Field label="Kind">
        <select
          value={kind}
          onChange={(e) => setKind(e.target.value as ProviderKindStr)}
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        >
          {KIND_OPTIONS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
        <span className="text-[11px] text-text-3 mt-1 block">{kindMeta.hint}</span>
      </Field>
      <Field label="Base URL">
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.openai.com/v1"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
      </Field>
      <Field
        label="API key env var"
        hint="Env var the workspace will read for auth. Leave blank for no-auth endpoints (local llama.cpp / Ollama)."
      >
        <input
          value={apiKeyEnv}
          onChange={(e) => setApiKeyEnv(e.target.value)}
          placeholder="OPENAI_API_KEY"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
      </Field>
      <div className="flex items-center gap-3 pt-1">
        <button
          type="submit"
          disabled={!submittable || add.isPending}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors"
        >
          {add.isPending ? "Adding…" : "Add provider"}
        </button>
        {add.isError ? (
          <span className="text-[12px] text-danger">
            {errorMessage(add.error)}
          </span>
        ) : null}
      </div>
    </form>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="text-[12px] text-text-2 mb-1 block">{label}</span>
      {children}
      {hint ? (
        <span className="text-[11px] text-text-3 mt-1 block">{hint}</span>
      ) : null}
    </label>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
