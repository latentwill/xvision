import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  agentProfileKeys,
  CANONICAL_AGENT_PROFILES,
  listAgentProfiles,
  updateAgentProfile,
} from "@/api/eval-review";
import type { AgentProfile } from "@/api/eval-review";
import { listProviders, settingsKeys } from "@/api/settings";
import { ApiError } from "@/api/client";
import { ModelPicker } from "@/components/ModelPicker";
import type { ProviderRow } from "@/api/types.gen/ProviderRow";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";

export function AgentPicker({
  selected,
  busy,
  onSelect,
}: {
  selected: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
}) {
  const qc = useQueryClient();
  const profilesQuery = useQuery({
    queryKey: agentProfileKeys.list(),
    queryFn: listAgentProfiles,
  });
  const providersQuery = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });

  const defaultId =
    selected ??
    CANONICAL_AGENT_PROFILES[1]?.id ??
    CANONICAL_AGENT_PROFILES[0].id;
  const [profileId, setProfileId] = useState(defaultId);
  const [provider, setProvider] = useState<string | null>(null);
  const [model, setModel] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);

  const profilesById = useMemo(
    () =>
      new Map<string, AgentProfile>(
        (profilesQuery.data ?? []).map((p) => [p.id, p]),
      ),
    [profilesQuery.data],
  );
  const live = profilesById.get(profileId) ?? null;
  const providerRows = useMemo(
    () => providersQuery.data?.providers ?? [],
    [providersQuery.data?.providers],
  );

  useEffect(() => {
    if (selected) setProfileId(selected);
  }, [selected]);

  useEffect(() => {
    const resolved = defaultReviewModel(providerRows);
    setProvider(resolved?.provider ?? null);
    setModel(resolved?.model ?? "");
    setSystemPrompt(live?.system_prompt ?? "");
    setLocalError(null);
  }, [live?.id, live?.system_prompt, providerRows]);

  const patchProfile = useMutation({
    mutationFn: ({
      id,
      patch,
    }: {
      id: string;
      patch: Partial<Pick<AgentProfile, "provider" | "model" | "system_prompt">>;
    }) => updateAgentProfile(id, patch),
    onSuccess: (updated) => {
      qc.setQueryData<AgentProfile[]>(agentProfileKeys.list(), (prev) => {
        const rows = prev ?? [];
        if (rows.length === 0) return [updated];
        return rows.some((p) => p.id === updated.id)
          ? rows.map((p) => (p.id === updated.id ? updated : p))
          : [...rows, updated];
      });
    },
  });

  async function generateWithSelectedProfile() {
    setLocalError(null);
    const patch: Partial<Pick<AgentProfile, "provider" | "model" | "system_prompt">> = {};
    if (provider && provider !== live?.provider) patch.provider = provider;
    if (model && model !== live?.model) patch.model = model;
    if (systemPrompt.trim() && systemPrompt !== live?.system_prompt) {
      patch.system_prompt = systemPrompt;
    }
    if (Object.keys(patch).length > 0) {
      try {
        await patchProfile.mutateAsync({ id: profileId, patch });
      } catch (err) {
        setLocalError(describeError(err).message);
        return;
      }
    }
    onSelect(profileId);
  }

  const profileLabel =
    CANONICAL_AGENT_PROFILES.find((p) => p.id === profileId)?.label ??
    live?.name ??
    profileId;
  const isSaving = patchProfile.isPending;

  return (
    <div className="space-y-3">
      <label className="flex flex-col gap-1 text-[12px] text-text-3">
        Review model
        <ModelPicker
          rows={providerRows}
          loading={providersQuery.isPending}
          provider={provider}
          model={model}
          onChange={(nextProvider, nextModel) => {
            setProvider(nextProvider);
            setModel(nextModel);
            setLocalError(null);
          }}
          className="w-full"
          ariaLabel="Review model"
          emptyHint="No enabled review models"
        />
      </label>

      <div className="flex flex-col gap-1 text-[12px] text-text-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span>Review prompt</span>
          <SignalSelectMenu
            ariaLabel="Review prompt preset"
            value={profileId}
            options={CANONICAL_AGENT_PROFILES.map((profile) => ({
              value: profile.id,
              label: profile.label,
            }))}
            onChange={setProfileId}
            disabled={busy || isSaving}
            className="bg-bg"
            minWidth={220}
          />
        </div>
        <textarea
          aria-label="Review prompt"
          value={systemPrompt}
          onChange={(e) => {
            setSystemPrompt(e.target.value);
            setLocalError(null);
          }}
          rows={4}
          className="w-full bg-bg border border-border rounded-sm px-2 py-2 text-text text-[12px] font-mono leading-relaxed"
          placeholder={`Prompt for ${profileLabel}`}
        />
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={generateWithSelectedProfile}
          disabled={busy || isSaving || !profileId || !provider || !model}
          className="px-3 py-1.5 rounded-sm text-[12px] border border-gold bg-gold text-bg font-medium disabled:opacity-50 motion-safe:active:scale-[0.96]"
        >
          {busy || patchProfile.isPending ? "Generating..." : `Generate review`}
        </button>
        {profilesQuery.isError ? (
          <span className="text-[11px] text-warn">
            Review presets could not be loaded.
          </span>
        ) : null}
      </div>

      {(localError || patchProfile.isError) && (
        <div
          role="alert"
          data-testid="agent-profile-save-error"
          className="border border-danger/40 rounded-sm p-2 text-danger text-[12px]"
        >
          {localError ?? describeError(patchProfile.error).message}
        </div>
      )}
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

function defaultReviewModel(
  providers: ProviderRow[],
): { provider: string; model: string } | null {
  for (const row of providers) {
    const model = row.enabled_models[0];
    if (model) return { provider: row.name, model };
  }
  return null;
}
