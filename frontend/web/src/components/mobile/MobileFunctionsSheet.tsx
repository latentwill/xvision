import { useNavigate } from "react-router-dom";

import { Icon, type IconName } from "@/components/primitives/Icon";
import { useUi } from "@/stores/ui";

type Action = {
  label: string;
  summary: string;
  icon: IconName;
  href?: string;
  disabled?: boolean;
};

const CREATE: Action[] = [
  { label: "New strategy", summary: "Create a blank draft", icon: "code", href: "/strategies/new" },
  { label: "Draft variant", summary: "Fork an existing strategy", icon: "branch", href: "/strategies" },
  { label: "Run backtest", summary: "Pick scenario and horizon", icon: "play", href: "/eval-runs" },
  { label: "Journal note", summary: "Capture a finding", icon: "book", disabled: true },
];

const INSPECT: Action[] = [
  { label: "Open a run", summary: "Chart, ledger, findings", icon: "bars", href: "/eval-runs" },
  { label: "Compare runs", summary: "Select from eval list", icon: "sliders", href: "/eval-runs" },
  { label: "Findings library", summary: "Coming with findings polish", icon: "pulse", disabled: true },
];

const LIVE: Action[] = [
  { label: "Deploy to paper", summary: "Paper surface pending", icon: "play", disabled: true },
  { label: "Pause / resume", summary: "Live daemon pending", icon: "flame", disabled: true },
];

export function MobileFunctionsSheet() {
  const open = useUi((s) => s.mobileFunctionsOpen);
  const setOpen = useUi((s) => s.setMobileFunctionsOpen);
  const setCmdkOpen = useUi((s) => s.setCmdkOpen);
  const navigate = useNavigate();

  if (!open) return null;

  const runAction = (action: Action) => {
    if (action.disabled) return;
    setOpen(false);
    if (action.href) navigate(action.href);
  };

  return (
    <div className="fixed inset-0 z-50 md:hidden">
      <button
        type="button"
        className="absolute inset-0 bg-black/55 backdrop-blur-[2px]"
        aria-label="Close all functions"
        onClick={() => setOpen(false)}
      />
      <section className="absolute left-0 right-0 bottom-0 max-h-[86vh] bg-surface-card border-t border-border rounded-t-[18px] flex flex-col">
        <div className="self-center w-9 h-1 rounded-full bg-border-strong mt-2 mb-1" />
        <div className="flex items-center justify-between px-4 py-3">
          <h2 className="m-0 font-sans font-medium text-[22px] text-text">
            All functions
          </h2>
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              setCmdkOpen(true);
            }}
            className="w-9 h-9 rounded-full flex items-center justify-center text-text-2 hover:text-text hover:bg-surface-hover"
            aria-label="Search commands"
          >
            <Icon name="search" size={16} />
          </button>
        </div>
        <div className="overflow-y-auto px-3 pb-[max(1.25rem,env(safe-area-inset-bottom))] flex flex-col gap-4">
          <ActionGrid label="Create" actions={CREATE} onAction={runAction} />
          <ActionList label="Inspect" actions={INSPECT} onAction={runAction} />
          <ActionList label="Live" actions={LIVE} onAction={runAction} />
        </div>
      </section>
    </div>
  );
}

function ActionGrid({
  label,
  actions,
  onAction,
}: {
  label: string;
  actions: Action[];
  onAction: (action: Action) => void;
}) {
  return (
    <section>
      <GroupLabel>{label}</GroupLabel>
      <div className="grid grid-cols-2 gap-2">
        {actions.map((action, index) => (
          <button
            key={action.label}
            type="button"
            disabled={action.disabled}
            onClick={() => onAction(action)}
            className={[
              "text-left flex flex-col gap-2 p-3.5 rounded-lg border bg-surface-elev",
              index === 0
                ? "border-gold/35"
                : "border-border-soft",
              action.disabled
                ? "opacity-45 cursor-not-allowed"
                : "hover:border-border-strong",
            ].join(" ")}
          >
            <span className="w-7 h-7 rounded-full border border-border-strong flex items-center justify-center text-text">
              <Icon name={action.icon} size={14} />
            </span>
            <span className="text-[13.5px] font-medium text-text">
              {action.label}
            </span>
            <span className="text-[11.5px] leading-snug text-text-3">
              {action.summary}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}

function ActionList({
  label,
  actions,
  onAction,
}: {
  label: string;
  actions: Action[];
  onAction: (action: Action) => void;
}) {
  return (
    <section>
      <GroupLabel>{label}</GroupLabel>
      <div className="flex flex-col gap-1.5">
        {actions.map((action) => (
          <button
            key={action.label}
            type="button"
            disabled={action.disabled}
            onClick={() => onAction(action)}
            className={[
              "w-full flex items-center gap-3 p-3 rounded-lg border border-border-soft bg-surface-elev text-left",
              action.disabled
                ? "opacity-45 cursor-not-allowed"
                : "hover:border-border-strong",
            ].join(" ")}
          >
            <span className="w-7 h-7 rounded-full bg-surface-panel flex items-center justify-center text-text-2">
              <Icon name={action.icon} size={14} />
            </span>
            <span className="flex-1 min-w-0">
              <span className="block text-[13.5px] text-text">
                {action.label}
              </span>
              <span className="block text-[11.5px] text-text-3 truncate">
                {action.summary}
              </span>
            </span>
            <Icon name="chevR" size={14} className="text-text-3" />
          </button>
        ))}
      </div>
    </section>
  );
}

function GroupLabel({ children }: { children: string }) {
  return (
    <div className="px-1 pb-1.5 text-[11px] uppercase tracking-wider font-mono text-text-3">
      {children}
    </div>
  );
}
