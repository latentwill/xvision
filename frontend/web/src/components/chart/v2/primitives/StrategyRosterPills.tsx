import { useState } from "react";

export type StrategyRosterItem = {
  id: string;
  label: string;
  color: string;
  active: boolean;
};

type Props = {
  items: StrategyRosterItem[];
  canRemove: boolean;
  onToggle: (id: string) => void;
  onRemove: (id: string) => void;
  onAdd: (id: string) => void;
};

export function StrategyRosterPills({
  items,
  canRemove,
  onToggle,
  onRemove,
  onAdd,
}: Props) {
  const [draft, setDraft] = useState("");

  return (
    <div className="flex flex-wrap items-center gap-2" data-testid="strategy-roster-pills">
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          onClick={() => onToggle(item.id)}
          className="inline-flex h-8 items-center gap-2 rounded-sm border px-2.5 text-[12px] text-text transition-colors"
          style={{
            background: item.active ? `${item.color}14` : "var(--surface-card)",
            borderColor: item.active ? `${item.color}55` : "var(--border)",
          }}
          title={item.id}
        >
          <span className="h-2 w-2 rounded-full" style={{ background: item.color }} aria-hidden />
          <span className="max-w-[180px] truncate">{item.label}</span>
          <span
            aria-hidden
            className={`font-mono ${canRemove ? "text-text-3" : "text-text-3/40"}`}
            onClick={(event) => {
              event.stopPropagation();
              onRemove(item.id);
            }}
          >
            x
          </span>
        </button>
      ))}
      <form
        className="ml-auto flex h-8 min-w-[220px] items-center overflow-hidden rounded-sm border border-border bg-surface-card"
        onSubmit={(event) => {
          event.preventDefault();
          onAdd(draft);
          setDraft("");
        }}
      >
        <input
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="Add run id"
          className="h-full min-w-0 flex-1 bg-transparent px-2 font-mono text-[12px] text-text outline-none placeholder:text-text-3"
          aria-label="Add run id"
        />
        <button
          type="submit"
          className="h-full border-l border-border px-2 font-mono text-[13px] text-text-2 hover:text-text"
          aria-label="Add run"
        >
          +
        </button>
      </form>
    </div>
  );
}
