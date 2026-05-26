// AgentDetailTabs — top-level tab container for `/agents/<id>`.
//
// V2D v1.1 introduces a per-agent Memory surface; the legacy
// AgentForm-only edit page is wrapped here so we can host a second
// tab without touching AgentForm.tsx (which is owned by the wider
// agent-editing surface and outside this contract's allowed_paths).
//
// Tabs:
//   - Configuration → existing AgentForm
//   - Memory        → MemoryTab (Patterns + Observations sub-tabs)
//
// New-agent flow (no `agentId` yet) keeps the bare AgentForm — there's
// no memory namespace until the agent exists.
//
// Phase 4 deep-link support: `?tab=memory` selects the Memory tab on
// mount (the eval-review MemoryPanel's "Open Pattern" deep-link drops
// the operator here), and `?pattern=<id>` is forwarded to the surface
// so the matching row highlights and scrolls into view.

import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";

import { AgentForm } from "./AgentForm";
import { MemoryTab } from "./MemoryTab";
import { AgentDiagnosticsView } from "@/components/diagnostics/AgentDiagnosticsView";
import type { AgentSlot } from "@/api/agents";

type Tab = "configuration" | "diagnostics" | "memory";

function tabFromParam(param: string | null): Tab {
  if (param === "memory") return "memory";
  if (param === "diagnostics") return "diagnostics";
  return "configuration";
}

export function AgentDetailTabs({
  agentId,
  initialSlots,
}: {
  agentId?: string;
  initialSlots?: AgentSlot[];
}) {
  const [params] = useSearchParams();
  const [tab, setTab] = useState<Tab>(() => tabFromParam(params.get("tab")));

  // Re-sync the active tab when the URL changes (e.g. browser back/forward,
  // or another component pushes a new query string).
  useEffect(() => {
    setTab(tabFromParam(params.get("tab")));
  }, [params]);

  const highlightPatternId = params.get("pattern");

  // Pre-save state — only the form is meaningful. The Memory tab
  // needs an agent_id to scope its namespace, so we hide it until the
  // agent is persisted.
  if (!agentId) {
    return <AgentForm initialSlots={initialSlots} />;
  }

  return (
    <div className="flex flex-col gap-4">
      <TabBar value={tab} onChange={setTab} />
      {tab === "configuration" ? (
        <AgentForm agentId={agentId} initialSlots={initialSlots} />
      ) : tab === "diagnostics" ? (
        <AgentDiagnosticsView agentId={agentId} />
      ) : (
        <MemoryTab agentId={agentId} highlightPatternId={highlightPatternId} />
      )}
    </div>
  );
}

function TabBar({
  value,
  onChange,
}: {
  value: Tab;
  onChange: (t: Tab) => void;
}) {
  const tabs: [Tab, string][] = [
    ["configuration", "Configuration"],
    ["diagnostics", "Diagnostics"],
    ["memory", "Memory"],
  ];
  return (
    <div
      role="tablist"
      aria-label="Agent detail tabs"
      className="flex gap-4 border-b border-border"
    >
      {tabs.map(([t, label]) => (
        <button
          key={t}
          type="button"
          role="tab"
          aria-selected={value === t}
          onClick={() => onChange(t)}
          className={`pb-2 -mb-px border-b-2 text-[13px] font-medium transition-colors ${
            value === t
              ? "border-gold text-text"
              : "border-transparent text-text-3 hover:text-text-2"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
