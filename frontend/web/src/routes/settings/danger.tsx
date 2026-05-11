import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import {
  dangerFactoryReset,
  dangerWipeDb,
} from "@/api/settings";
import type {
  FactoryResetReport,
  WipeDbReport,
} from "@/api/types.gen";

const CONFIRM_PHRASE = "DELETE";

export function SettingsDangerRoute() {
  return (
    <div className="space-y-5">
      <div className="rounded border border-danger/40 bg-danger/5 dark:bg-danger/10 px-4 py-3">
        <div className="text-[13px] text-danger font-medium">
          ⚠ Destructive operations
        </div>
        <p className="m-0 mt-1 text-text-2 text-[12px] leading-snug">
          Every action on this page is irreversible. Each requires you to type{" "}
          <code className="font-mono">{CONFIRM_PHRASE}</code> to confirm and is
          audit-logged before it runs.
        </p>
      </div>

      <DangerSection<WipeDbReport>
        title="Wipe local database"
        description={
          <>
            Deletes every row in <code className="font-mono">xvn.db</code>{" "}
            except the <code className="font-mono">api_audit</code> trail, so
            the record of <em>this</em> wipe survives. Strategy bundles on
            disk, the config TOML, and any signing keys are untouched.
          </>
        }
        actionLabel="Wipe database"
        mutationFn={dangerWipeDb}
        renderSuccess={(r) => (
          <ul className="m-0 p-0 list-none text-[12px] text-text-2">
            <li className="mb-1">
              <span className="text-text">{r.total_rows_deleted}</span> row
              {r.total_rows_deleted === 1 ? "" : "s"} cleared across{" "}
              <span className="text-text">{r.tables.length}</span> table
              {r.tables.length === 1 ? "" : "s"}.
            </li>
            {r.tables.map((t) => (
              <li
                key={t.table}
                className="flex justify-between border-t border-border-soft py-1 last:border-b-0"
              >
                <code className="font-mono">{t.table}</code>
                <span className="text-text-3">{t.rows_deleted}</span>
              </li>
            ))}
          </ul>
        )}
      />

      <DangerSection<FactoryResetReport>
        title="Factory reset (delete XVN_HOME)"
        description={
          <>
            Deletes the entire <code className="font-mono">$XVN_HOME</code>{" "}
            directory and recreates it empty. Strategy bundles, eval runs,
            chat sessions, search index — all gone. An audit line is mirrored
            to a sibling log file <em>outside</em> the home directory before
            the wipe runs, so the trail survives.
          </>
        }
        actionLabel="Factory reset"
        mutationFn={dangerFactoryReset}
        renderSuccess={(r) => (
          <div className="text-[12px] text-text-2">
            <code className="font-mono text-text">{r.xvn_home}</code> wiped and
            recreated.
            <div className="mt-1 text-text-3">
              Audit trail mirrored to{" "}
              <code className="font-mono">{r.audit_log_path}</code>
            </div>
          </div>
        )}
      />
    </div>
  );
}

function DangerSection<T>({
  title,
  description,
  actionLabel,
  mutationFn,
  renderSuccess,
}: {
  title: string;
  description: React.ReactNode;
  actionLabel: string;
  mutationFn: () => Promise<T>;
  renderSuccess: (data: T) => React.ReactNode;
}) {
  const [phrase, setPhrase] = useState("");
  const armed = phrase === CONFIRM_PHRASE;
  const m = useMutation({
    mutationFn,
    onSuccess: () => setPhrase(""),
  });

  return (
    <Card className="p-5 border-danger/30">
      <h3 className="m-0 font-serif font-medium text-[18px] tracking-tight text-text">
        {title}
      </h3>
      <p className="m-0 mt-2 text-text-2 text-[13px] leading-snug">
        {description}
      </p>

      <div className="mt-4 flex items-center gap-3">
        <input
          type="text"
          value={phrase}
          onChange={(e) => setPhrase(e.target.value)}
          placeholder={`Type ${CONFIRM_PHRASE} to confirm`}
          className="flex-1 max-w-[260px] bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
        <button
          type="button"
          onClick={() => m.mutate()}
          disabled={!armed || m.isPending}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-danger/60 text-danger hover:bg-danger/10 disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:bg-transparent"
        >
          {m.isPending ? "Working…" : actionLabel}
        </button>
      </div>

      {m.isError ? (
        <div className="mt-3 text-[12px] text-danger">
          <code className="font-mono">{errorMessage(m.error)}</code>
        </div>
      ) : null}

      {m.isSuccess && m.data ? (
        <div className="mt-4 pt-3 border-t border-border-soft">
          <div className="text-[11px] uppercase tracking-wider text-text-3 mb-2">
            Result
          </div>
          {renderSuccess(m.data)}
        </div>
      ) : null}
    </Card>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
