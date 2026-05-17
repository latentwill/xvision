// frontend/web/src/features/agent-runs/HaltStrategyButton.tsx
import { useState } from "react";

export function HaltStrategyButton({
  strategyName,
  onHalt,
}: {
  strategyName: string;
  onHalt: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const matches = typed === strategyName;
  return (
    <div className="flex items-center gap-2">
      {!open ? (
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="px-2 py-1 border border-red-500/60 text-red-300 rounded-sm text-[11px] hover:bg-red-950/40"
        >
          ⏹ halt strategy
        </button>
      ) : (
        <>
          <input
            type="text"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={`type ${strategyName} to confirm`}
            className="px-2 py-1 bg-surface-card border border-border rounded-sm text-[11px] font-mono w-56"
          />
          <button
            type="button"
            disabled={!matches}
            onClick={() => { onHalt(); setOpen(false); setTyped(""); }}
            className="px-2 py-1 border border-red-500/60 text-red-200 bg-red-950/60 rounded-sm text-[11px] disabled:opacity-40"
          >
            halt
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); setTyped(""); }}
            className="px-2 py-1 text-text-3 text-[11px] hover:text-text"
          >
            cancel
          </button>
        </>
      )}
    </div>
  );
}
