import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  agentProfileKeys,
  CANONICAL_AGENT_PROFILES,
  listAgentProfiles,
  updateAgentProfile,
} from "@/api/eval-review";
import type { AgentProfile } from "@/api/eval-review";
import { listProviders } from "@/api/settings";
import { ApiError } from "@/api/client";

/// Pill picker. One button per agent profile. Each pill has an
/// inline expand affordance ("⚙") that opens a provider+model
/// selector docked below the pill row. The selector PATCHes
/// `/api/eval/agent-profiles/:id` so the next "Review with X" click
/// dispatches against the operator's actual provider — not the
/// migration-seeded `anthropic` pin that breaks for openrouter-only
/// users.
///
/// Picker keeps the static `CANONICAL_AGENT_PROFILES` list as a
/// label/blurb fallback while the profiles query is pending — the
/// four ids are stable per migration 016.
export function AgentPicker({
  selected,
  busy,
  onSelect,
}: {
  selected: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
}) {
  const [editing, setEditing] = useState<string | null>(null);

  const profilesQuery = useQuery({
    queryKey: agentProfileKeys.list(),
    queryFn: listAgentProfiles,
  });

  const profilesById = new Map<string, AgentProfile>(
    (profilesQuery.data ?? []).map((p) => [p.id, p]),
  );

  return (
    <div>
      <div className="flex flex-wrap gap-2">
        {CANONICAL_AGENT_PROFILES.map((p) => {
          const isSelected = p.id === selected;
          const live = profilesById.get(p.id);
          const isEditing = editing === p.id;
          return (
            <div key={p.id} className="flex items-stretch">
              <button
                type="button"
                onClick={() => onSelect(p.id)}
                disabled={busy}
                aria-pressed={isSelected}
                title={p.blurb}
                className={[
                  "px-3 py-1.5 rounded-l-sm text-[12px] border transition-colors",
                  isSelected
                    ? "bg-gold border-gold text-bg font-medium"
                    : "border-border text-text-2 hover:border-gold/60 hover:text-text",
                  busy ? "opacity-50 cursor-wait" : "",
                ].join(" ")}
              >
                {p.label}
              </button>
              <button
                type="button"
                onClick={() => setEditing(isEditing ? null : p.id)}
                aria-label={`Edit provider for ${p.label}`}
                aria-expanded={isEditing}
                title={
                  live
                    ? `Provider: ${live.provider} · Model: ${live.model}`
                    : "Edit provider/model"
                }
                className={[
                  "px-2 rounded-r-sm text-[12px] border border-l-0 transition-colors",
                  isEditing
                    ? "bg-bg-2 border-gold text-text"
                    : "border-border text-text-3 hover:border-gold/60 hover:text-text",
                ].join(" ")}
              >
                ⚙
              </button>
            </div>
          );
        })}
      </div>
      {editing && (
        <ProfileEditor
          profileId={editing}
          profile={profilesById.get(editing) ?? null}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}

function ProfileEditor({
  profileId,
  profile,
  onClose,
}: {
  profileId: string;
  profile: AgentProfile | null;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const providersQuery = useQuery({
    queryKey: ["settings", "providers"],
    queryFn: listProviders,
  });

  const [provider, setProvider] = useState<string>(profile?.provider ?? "");
  const [model, setModel] = useState<string>(profile?.model ?? "");

  const saveMutation = useMutation({
    mutationFn: () => updateAgentProfile(profileId, { provider, model }),
    onSuccess: (updated) => {
      qc.setQueryData<AgentProfile[]>(agentProfileKeys.list(), (prev) =>
        (prev ?? []).map((p) => (p.id === updated.id ? updated : p)),
      );
      onClose();
    },
  });

  const providers = providersQuery.data?.providers ?? [];
  const activeProviderRow = providers.find((p) => p.name === provider) ?? null;
  // enabled_models is the operator's curated list (see ProviderRow doc
  // comment). When empty, we still show the current value as a free-text
  // input so the operator can save without first running through
  // Settings → Providers → Manage models.
  const enabledModels = activeProviderRow?.enabled_models ?? [];

  const { code, message } =
    saveMutation.isError ? describeError(saveMutation.error) : { code: "", message: "" };

  return (
    <div
      role="region"
      aria-label="Edit review-agent provider and model"
      className="mt-3 border border-border rounded-card p-3 bg-bg-2"
    >
      <div className="flex items-baseline justify-between mb-2">
        <span className="text-text-2 text-[12px]">
          {profile?.name ?? profileId}
        </span>
        <button
          type="button"
          onClick={onClose}
          className="text-text-3 text-[11px] underline decoration-dotted underline-offset-2 hover:text-text-2"
        >
          close
        </button>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <label className="flex flex-col gap-1 text-[12px] text-text-3">
          Provider
          <select
            value={provider}
            onChange={(e) => {
              setProvider(e.target.value);
              // Reset model when provider changes — model ids are
              // provider-scoped (e.g. `anthropic/claude-…` only makes
              // sense for OpenRouter).
              setModel("");
            }}
            disabled={providersQuery.isPending}
            className="bg-bg border border-border rounded-sm px-2 py-1 text-text text-[12px]"
          >
            <option value="">(none)</option>
            {providers.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name} · {p.kind}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-text-3">
          Model
          {enabledModels.length > 0 ? (
            <select
              value={model}
              onChange={(e) => setModel(e.target.value)}
              className="bg-bg border border-border rounded-sm px-2 py-1 text-text text-[12px]"
            >
              <option value="">(pick a model)</option>
              {enabledModels.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={
                activeProviderRow
                  ? "e.g. anthropic/claude-sonnet-4.5"
                  : "pick a provider first"
              }
              disabled={!activeProviderRow}
              className="bg-bg border border-border rounded-sm px-2 py-1 text-text text-[12px]"
            />
          )}
        </label>
      </div>
      {providersQuery.isError && (
        <div className="mt-2 text-danger text-[12px]">
          Couldn't load providers — fix the connection in Settings → Providers.
        </div>
      )}
      {saveMutation.isError && (
        <div
          role="alert"
          data-testid="agent-profile-save-error"
          className="mt-2 border border-danger/40 rounded-sm p-2 text-danger text-[12px]"
        >
          <span className="inline-flex items-center px-1.5 py-0.5 mr-2 rounded-sm text-[10px] uppercase tracking-wide border border-danger/40 bg-danger/10">
            {code}
          </span>
          {message}
        </div>
      )}
      <div className="mt-3 flex gap-2">
        <button
          type="button"
          onClick={() => saveMutation.mutate()}
          disabled={
            saveMutation.isPending ||
            !provider ||
            !model ||
            (provider === profile?.provider && model === profile?.model)
          }
          className="px-3 py-1.5 rounded-sm text-[12px] border border-gold bg-gold text-bg font-medium disabled:opacity-50"
        >
          {saveMutation.isPending ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          onClick={onClose}
          className="px-3 py-1.5 rounded-sm text-[12px] border border-border text-text-2 hover:border-gold/60 hover:text-text"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function describeError(error: unknown): { code: string; message: string } {
  if (error instanceof ApiError) {
    return { code: error.code, message: error.message };
  }
  if (error instanceof Error) {
    return { code: "error", message: error.message };
  }
  return { code: "error", message: String(error) };
}
