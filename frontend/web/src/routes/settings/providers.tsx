import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  addProvider,
  listProviders,
  removeProvider,
  setDefaultProvider,
  settingsKeys,
} from "@/api/settings";
import type {
  AddProviderRequest,
  ProviderRow,
} from "@/api/types.gen";

// Provider presets the form recognises. Each preset fills in a sensible
// (wire kind, name, base URL) tuple; the user only has to paste an API key.
// Pick "Custom" for anything else — DeepSeek/Groq/Together-style endpoints
// that don't have a preset, or self-hosted Ollama / vLLM / llama.cpp.
type KindOption = {
  value: string;
  label: string;
  wireKind: "anthropic" | "openai-compat";
  defaultName: string;
  defaultBaseUrl: string;
  isCustom: boolean;
  keyHelp: string;
};

const KIND_OPTIONS: ReadonlyArray<KindOption> = [
  {
    value: "anthropic",
    label: "Anthropic (Claude)",
    wireKind: "anthropic",
    defaultName: "anthropic",
    defaultBaseUrl: "https://api.anthropic.com",
    isCustom: false,
    keyHelp: "Starts with sk-ant-…",
  },
  {
    value: "openai",
    label: "OpenAI",
    wireKind: "openai-compat",
    defaultName: "openai",
    defaultBaseUrl: "https://api.openai.com/v1",
    isCustom: false,
    keyHelp: "Starts with sk-…",
  },
  {
    value: "deepseek",
    label: "DeepSeek",
    wireKind: "openai-compat",
    defaultName: "deepseek",
    // DeepSeek's OpenAI-compat root, per https://api-docs.deepseek.com.
    // We POST to `{base}/chat/completions` — no /v1 segment.
    defaultBaseUrl: "https://api.deepseek.com",
    isCustom: false,
    keyHelp: "Starts with sk-…",
  },
  {
    value: "groq",
    label: "Groq",
    wireKind: "openai-compat",
    defaultName: "groq",
    defaultBaseUrl: "https://api.groq.com/openai/v1",
    isCustom: false,
    keyHelp: "Starts with gsk_…",
  },
  {
    value: "openrouter",
    label: "OpenRouter",
    wireKind: "openai-compat",
    defaultName: "openrouter",
    defaultBaseUrl: "https://openrouter.ai/api/v1",
    isCustom: false,
    keyHelp: "Starts with sk-or-…",
  },
  {
    value: "custom",
    label: "Custom (Ollama, Together, vLLM, self-hosted, …)",
    wireKind: "openai-compat",
    defaultName: "",
    defaultBaseUrl: "",
    isCustom: true,
    keyHelp: "Leave blank for no-auth endpoints (local Ollama).",
  },
];

// Local-Ollama style endpoints don't need a key. Anything else does.
function keyRequired(meta: KindOption, baseUrl: string): boolean {
  if (meta.value === "custom" && /localhost|127\.0\.0\.1/.test(baseUrl)) {
    return false;
  }
  return true;
}

function isHttpUrl(s: string): boolean {
  return /^https?:\/\/.+/i.test(s);
}

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

  const promote = useMutation({
    mutationFn: (name: string) => setDefaultProvider(name),
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
          <div>
            <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
              LLM providers
            </h3>
            <p className="m-0 mt-1 text-text-3 text-[12px]">
              Models xvision can call for the intern, agent rail, and
              authoring wizard.
            </p>
          </div>
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
          Paste a provider's API key to enable it. Keys are stored on disk
          under <code className="font-mono">~/.xvn/secrets/providers.toml</code>{" "}
          (owner-only) — they never round-trip through this UI again.
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
            no providers yet — click <span className="text-text">+ Add provider</span> to start
          </div>
        ) : (
          <table className="w-full mt-2">
            <thead>
              <tr className="text-text-3 text-[11px] uppercase tracking-wider text-left">
                <th className="py-2 pr-3 font-normal">Name</th>
                <th className="py-2 pr-3 font-normal">Kind</th>
                <th className="py-2 pr-3 font-normal">Base URL</th>
                <th className="py-2 pr-3 font-normal text-right">Key</th>
                <th className="py-2 pr-0 font-normal text-right" />
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => (
                <ProviderRowView
                  key={p.name}
                  row={p}
                  canPromote={
                    rows.some(
                      (r) =>
                        r.name !== p.name &&
                        r.api_key_set &&
                        !r.synthetic,
                    )
                  }
                  onRemove={() => remove.mutate(p.name)}
                  onPromote={() => promote.mutate(p.name)}
                  removeError={
                    remove.variables === p.name && remove.isError
                      ? errorMessage(remove.error)
                      : null
                  }
                  removeBusy={
                    remove.variables === p.name && remove.isPending
                  }
                  promoteBusy={
                    promote.variables === p.name && promote.isPending
                  }
                  promoteError={
                    promote.variables === p.name && promote.isError
                      ? errorMessage(promote.error)
                      : null
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
  canPromote,
  onRemove,
  onPromote,
  removeError,
  removeBusy,
  promoteBusy,
  promoteError,
}: {
  row: ProviderRow;
  canPromote: boolean;
  onRemove: () => void;
  onPromote: () => void;
  removeError: string | null;
  removeBusy: boolean;
  promoteBusy: boolean;
  promoteError: string | null;
}) {
  // Default rows are locked from deletion because the intern slot needs
  // a target. Operators can switch the default to another configured
  // provider first (the "set as default" button below); that unlocks the
  // old default for removal.
  const locked = row.referenced_by_intern;
  const lockReason = locked
    ? "Workspace default — promote another provider first, then come back to remove this one."
    : null;
  return (
    <>
      <tr className="border-t border-border-soft align-middle">
        <td className="py-2 pr-3">
          <div className="flex items-center gap-2">
            <code className="font-mono text-[13px] text-text">{row.name}</code>
            {row.referenced_by_intern ? <Pill tone="gold">default</Pill> : null}
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
        <td className="py-2 pr-3 text-right">
          {!row.api_key_env ? (
            <span className="text-text-3 text-[12px]">no auth</span>
          ) : row.api_key_set ? (
            <span className="text-gold text-[12px]">● set</span>
          ) : (
            <span className="text-warn text-[12px]">○ missing</span>
          )}
        </td>
        <td className="py-2 pr-0 text-right">
          <div className="inline-flex items-center gap-2">
            {!row.referenced_by_intern && row.api_key_set ? (
              <button
                onClick={onPromote}
                disabled={promoteBusy}
                title="Make this the workspace default (intern provider)"
                className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-gold hover:border-gold disabled:opacity-30"
              >
                {promoteBusy ? "Switching…" : "Set as default"}
              </button>
            ) : null}
            <button
              onClick={onRemove}
              disabled={locked || removeBusy}
              title={lockReason ?? undefined}
              className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-danger hover:border-danger disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:text-text-2 disabled:hover:border-border"
            >
              {removeBusy ? "Removing…" : "Remove"}
            </button>
          </div>
        </td>
      </tr>
      {removeError ? (
        <tr className="border-t border-border-soft/40">
          <td colSpan={5} className="py-2 pr-0 text-[12px] text-danger">
            {removeError}
          </td>
        </tr>
      ) : null}
      {promoteError ? (
        <tr className="border-t border-border-soft/40">
          <td colSpan={5} className="py-2 pr-0 text-[12px] text-danger">
            {promoteError}
          </td>
        </tr>
      ) : null}
      {locked && !canPromote ? (
        <tr className="border-t border-border-soft/40">
          <td colSpan={5} className="py-2 pr-0 text-[12px] text-text-3">
            Add another provider with an API key, then you can demote this
            one and remove it.
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
  const [kindOption, setKindOption] = useState<string>(KIND_OPTIONS[0].value);
  const [name, setName] = useState<string>(KIND_OPTIONS[0].defaultName);
  const [baseUrl, setBaseUrl] = useState<string>(KIND_OPTIONS[0].defaultBaseUrl);
  const [apiKey, setApiKey] = useState("");

  const meta = KIND_OPTIONS.find((k) => k.value === kindOption) ?? KIND_OPTIONS[0];

  const add = useMutation({
    mutationFn: (req: AddProviderRequest) => addProvider(req),
    onSuccess: () => onAdded(),
  });

  const trimmedName = name.trim();
  const trimmedBaseUrl = baseUrl.trim();
  const trimmedKey = apiKey.trim();
  const needsKey = keyRequired(meta, trimmedBaseUrl);
  const errors: string[] = [];
  if (trimmedName === "") errors.push("name is required");
  if (trimmedBaseUrl === "") {
    errors.push("base URL is required");
  } else if (!isHttpUrl(trimmedBaseUrl)) {
    errors.push("base URL must start with http:// or https://");
  }
  if (needsKey && trimmedKey === "") errors.push("API key is required");
  const submittable = errors.length === 0;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (!submittable) return;
        // Surface client-side console logs so the user can see what was sent
        // when reporting issues — the previous flow was a black box.
        console.info("[providers] add", {
          name: trimmedName,
          kind: meta.wireKind,
          base_url: trimmedBaseUrl,
          api_key_set: trimmedKey !== "",
        });
        add.mutate({
          name: trimmedName,
          kind: meta.wireKind,
          base_url: trimmedBaseUrl,
          api_key_env: "",
          api_key: trimmedKey === "" ? undefined : apiKey,
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

      <Field label="Provider">
        <select
          value={kindOption}
          onChange={(e) => {
            const next = KIND_OPTIONS.find((k) => k.value === e.target.value);
            if (!next) return;
            setKindOption(next.value);
            setName(next.defaultName);
            setBaseUrl(next.defaultBaseUrl);
          }}
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text"
        >
          {KIND_OPTIONS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
      </Field>

      <Field
        label={needsKey ? "API key (required)" : "API key"}
        hint={meta.keyHelp}
      >
        <input
          type="password"
          autoFocus
          autoComplete="off"
          spellCheck={false}
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="paste key here"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
          required={needsKey}
        />
      </Field>

      {meta.isCustom ? (
        <Field
          label="Name"
          hint="lowercase, digits, hyphens. e.g. ollama, together."
        >
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. ollama"
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
          />
        </Field>
      ) : null}

      <Field
        label="Base URL"
        hint={
          meta.isCustom
            ? "The /v1 endpoint root."
            : "Pre-filled for this provider — override only for proxies or self-hosted gateways."
        }
      >
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.example.com/v1"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
      </Field>

      {errors.length > 0 ? (
        <ul className="m-0 pl-4 text-[12px] text-danger list-disc">
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      ) : null}
      <div className="flex items-center gap-3 pt-1">
        <button
          type="submit"
          disabled={!submittable || add.isPending}
          title={submittable ? "" : errors.join("; ")}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors"
        >
          {add.isPending ? "Saving…" : "Save provider"}
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
