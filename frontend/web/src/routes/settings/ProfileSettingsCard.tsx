import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import { getProfile, settingsKeys, updateProfile } from "@/api/settings";

/**
 * Settings → General "Your profile" card. Persists a display name / handle to
 * the backend (`/api/settings/profile`). The handle is stamped as the `creator`
 * on newly created strategies and can be applied to an existing strategy from
 * its detail page ("Use my handle"). QA: "allow creator field to be updated
 * with the user profile".
 */
export function ProfileSettingsCard() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: settingsKeys.profile(), queryFn: getProfile });
  const [handle, setHandle] = useState("");
  const [dirty, setDirty] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Seed the input from the persisted value once it loads (until the operator
  // starts editing).
  useEffect(() => {
    if (!dirty && q.data) {
      setHandle(q.data.display_name ?? "");
    }
  }, [q.data, dirty]);

  const save = useMutation({
    mutationFn: () => updateProfile({ display_name: handle.trim() }),
    onSuccess: (report) => {
      qc.setQueryData(settingsKeys.profile(), report);
      qc.invalidateQueries({ queryKey: settingsKeys.profile() });
      setErrorMsg(null);
      setDirty(false);
    },
    onError: (err) => {
      setErrorMsg(
        err instanceof ApiError
          ? `${err.code}: ${err.message}`
          : err instanceof Error
            ? err.message
            : String(err),
      );
    },
  });

  const persisted = q.data?.display_name ?? "";

  return (
    <Card className="p-5">
      <div className="mb-4">
        <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
          Your profile
        </h3>
        <p className="m-0 mt-1 max-w-2xl text-[12px] leading-snug text-text-3">
          A display name / handle that identifies you as the creator of
          strategies you build. New strategies are stamped with this handle, and
          you can apply it to existing strategies from their detail page.
        </p>
      </div>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
        className="space-y-3"
      >
        <div>
          <label className="block text-[12px] text-text-2 mb-1">
            Display name
          </label>
          <input
            type="text"
            autoComplete="off"
            spellCheck={false}
            value={handle}
            onChange={(e) => {
              setHandle(e.target.value);
              setDirty(true);
            }}
            placeholder="@yourhandle"
            className="w-full max-w-sm px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono placeholder:text-text-3 focus:outline-none focus:border-text-3"
          />
        </div>
        {errorMsg ? (
          <p className="m-0 text-[12px] text-danger font-mono">{errorMsg}</p>
        ) : null}
        <div className="flex items-center gap-3 pt-1">
          <button
            type="submit"
            disabled={save.isPending || handle.trim() === persisted}
            className="px-3 py-1.5 rounded text-[13px] font-medium border border-gold text-gold hover:bg-gold/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {save.isPending ? "Saving…" : "Save"}
          </button>
          {save.isSuccess && !dirty ? (
            <span className="text-[12px] text-text-3">Saved.</span>
          ) : null}
        </div>
      </form>
    </Card>
  );
}
