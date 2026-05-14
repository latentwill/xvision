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
    <div className="flex flex-col xl:flex-row xl:items-start xl:justify-between gap-3 xl:gap-8 mb-5 xl:mb-7">
      <div className="min-w-0">
        <h1 className="m-0 mb-1 font-serif font-medium text-[30px] xl:text-[38px] tracking-tight leading-none">
          {title}
        </h1>
        {sub ? <div className="text-text-2 text-sm">{sub}</div> : null}
      </div>

      <div className="flex items-center gap-3 min-w-0 w-full xl:w-auto">
        <HealthPill />

        <button
          type="button"
          onClick={() => setCmdkOpen(true)}
          aria-label="Open command palette"
          className="flex items-center gap-2.5 min-w-0 flex-1 xl:flex-none xl:w-[380px] max-w-full px-3 py-2 bg-surface-elev border border-border rounded text-text-3 text-[13px] hover:border-text-3 hover:text-text-2 transition-colors text-left"
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
