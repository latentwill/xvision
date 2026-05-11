import { HealthPill } from "./HealthPill";
import { useUi } from "@/stores/ui";
import { modKeyLabel } from "@/lib/platform";

export function Topbar({
  title,
  sub,
  cmdkPlaceholder = "Jump to anything…",
}: {
  title: string;
  sub?: string;
  cmdkPlaceholder?: string;
}) {
  const setCmdkOpen = useUi((s) => s.setCmdkOpen);
  return (
    <div className="flex items-start justify-between gap-8 mb-7">
      <div className="min-w-0">
        <h1 className="m-0 mb-1 font-serif font-medium text-[38px] tracking-tight leading-none">
          {title}
        </h1>
        {sub ? <div className="text-text-2 text-sm">{sub}</div> : null}
      </div>

      <div className="flex items-center gap-3">
        <HealthPill />

        <button
          type="button"
          onClick={() => setCmdkOpen(true)}
          aria-label="Open command palette"
          className="flex items-center gap-2.5 w-[380px] px-3 py-2 bg-surface-elev border border-border rounded text-text-3 text-[13px] hover:border-text-3 hover:text-text-2 transition-colors text-left"
        >
          <span className="inline-flex items-center px-1.5 py-px border border-border-strong rounded-sm font-mono text-[11px] text-text-2">
            {modKeyLabel()} K
          </span>
          <span className="flex-1 truncate">{cmdkPlaceholder}</span>
        </button>
      </div>
    </div>
  );
}
