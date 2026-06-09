import { useState } from "react";
import { useDeleteSchedule, useSchedule, useUpsertSchedule, type UpsertScheduleRequest } from "../api";

/**
 * ScheduleStrip — compact inline strip for the optimizer's scheduled-run config.
 *
 * - No schedule: shows "No scheduled run · Set one" with an inline accordion form
 * - Has schedule: shows "Next: HH:MM · strategy_id" + enable/disable toggle + Edit button
 *
 * No-popup rule: edit form is an inline accordion, never a modal/sheet/popover.
 */
export function ScheduleStrip() {
  const { data: schedule } = useSchedule();
  const { mutate: upsertSchedule, isPending } = useUpsertSchedule();
  const { mutate: deleteSchedule, isPending: isDeleting } = useDeleteSchedule();

  const [open, setOpen] = useState(false);
  const [timeLocal, setTimeLocal] = useState("");
  const [strategyId, setStrategyId] = useState("");

  function openForm(prefill?: { time_local: string; strategy_id: string }) {
    setTimeLocal(prefill?.time_local ?? "");
    setStrategyId(prefill?.strategy_id ?? "");
    setOpen(true);
  }

  function closeForm() {
    setOpen(false);
  }

  function handleSave() {
    upsertSchedule(
      {
        enabled: schedule?.enabled ?? true,
        time_local: timeLocal,
        strategy_id: strategyId,
      } satisfies UpsertScheduleRequest,
      { onSuccess: () => closeForm() },
    );
  }

  function handleToggle() {
    if (!schedule) return;
    upsertSchedule({
      enabled: !schedule.enabled,
      time_local: schedule.time_local,
      strategy_id: schedule.strategy_id,
    });
  }

  return (
    <div className="flex flex-col gap-0.5 px-4 py-2 rounded-md bg-surface-card border border-border/60 text-[12px] font-mono text-text-2">
      {/* ─── Status line ─────────────────────────────────────────────────────── */}
      {schedule ? (
        <div className="flex items-center gap-3 flex-wrap">
          {/* Enable/disable toggle */}
          <label className="flex items-center gap-1.5 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={schedule.enabled}
              onChange={handleToggle}
              disabled={isPending}
              className="accent-accent h-3 w-3 cursor-pointer"
              aria-label="Enable scheduled run"
            />
          </label>

          {/* Next-run summary */}
          <span>
            Next:{" "}
            <span className="text-text font-semibold">{schedule.time_local}</span>
            {" · "}
            <span className="text-text">{schedule.strategy_id}</span>
          </span>

          {/* Edit button */}
          <button
            type="button"
            onClick={() => (open ? closeForm() : openForm(schedule))}
            className="ml-auto rounded border border-border/60 px-2 py-0.5 text-[11px] text-text-2 hover:bg-surface-elev/40 transition-colors"
          >
            {open ? "Close" : "Edit"}
          </button>

          {/* Delete button */}
          <button
            type="button"
            onClick={() => deleteSchedule(schedule.id)}
            disabled={isPending || isDeleting}
            className="rounded border border-danger/40 px-2 py-0.5 text-[11px] text-danger hover:bg-danger/[0.06] disabled:opacity-40 transition-colors"
          >
            {isDeleting ? "Removing…" : "Remove"}
          </button>
        </div>
      ) : (
        <span>
          No scheduled run{" · "}
          <button
            type="button"
            onClick={() => (open ? closeForm() : openForm())}
            className="underline text-text-2 hover:text-text transition-colors bg-transparent border-0 p-0 cursor-pointer font-mono text-[12px]"
          >
            {open ? "Cancel" : "Set one"}
          </button>
        </span>
      )}

      {/* ─── Inline accordion form ────────────────────────────────────────────── */}
      {open && (
        <div className="mt-1.5 pt-2 border-t border-border/40 flex flex-col gap-2">
          <div className="flex items-center gap-3 flex-wrap">
            {/* Time input */}
            <label className="flex items-center gap-1.5 text-[11px] text-text-3">
              Time (HH:MM)
              <input
                type="text"
                value={timeLocal}
                onChange={(e) => setTimeLocal(e.target.value)}
                placeholder="21:00"
                className="ml-1 rounded border border-border bg-surface-elev px-2 py-0.5 text-[12px] text-text w-20 font-mono"
                aria-label="Scheduled time"
                pattern="^([01]\d|2[0-3]):[0-5]\d$"
              />
            </label>

            {/* Strategy id input */}
            <label className="flex items-center gap-1.5 text-[11px] text-text-3">
              Strategy
              <input
                type="text"
                value={strategyId}
                onChange={(e) => setStrategyId(e.target.value)}
                placeholder="strategy-id"
                className="ml-1 rounded border border-border bg-surface-elev px-2 py-0.5 text-[12px] text-text w-36 font-mono"
                aria-label="Strategy ID"
              />
            </label>
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleSave}
              disabled={isPending || !timeLocal || !strategyId}
              className="rounded bg-accent px-2.5 py-1 text-[11px] font-medium text-on-accent hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Save
            </button>
            <button
              type="button"
              onClick={closeForm}
              className="rounded border border-border/60 px-2.5 py-1 text-[11px] text-text-2 hover:bg-surface-elev/40 transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
