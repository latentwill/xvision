// /agents/new and /agents/:id — single route component handling both
// modes (matches the Inspector pattern from /authoring + /authoring/:id).

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";

import {
  agentKeys,
  listAgentTemplates,
  type AgentSlot,
  type AgentTemplate,
} from "@/api/agents";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { AgentForm } from "@/components/agent/AgentForm";

export function AgentsEditRoute() {
  const params = useParams<{ id?: string }>();
  const agentId = params.id;
  const isNew = !agentId;

  // Seed slots from a picked template (new mode only).
  const [seededSlots, setSeededSlots] = useState<AgentSlot[] | null>(null);
  const [pickedTemplateId, setPickedTemplateId] = useState<string | null>(null);

  return (
    <>
      <Topbar
        title={isNew ? "New agent" : "Agent"}
        sub={isNew ? "Single-slot draft" : agentId}
      />
      <Link
        to="/agents"
        className="inline-flex items-center gap-1.5 text-[12px] text-text-2 hover:text-text mb-3"
      >
        ← Back to agents
      </Link>

      {isNew && !pickedTemplateId ? (
        <TemplatePicker
          onPick={(t) => {
            setSeededSlots(t.slots);
            setPickedTemplateId(t.id);
          }}
          onSkip={() => setPickedTemplateId("__blank__")}
        />
      ) : (
        <AgentForm agentId={agentId} initialSlots={seededSlots ?? undefined} />
      )}
    </>
  );
}

function TemplatePicker({
  onPick,
  onSkip,
}: {
  onPick: (t: AgentTemplate) => void;
  onSkip: () => void;
}) {
  const q = useQuery({
    queryKey: agentKeys.templates(),
    queryFn: listAgentTemplates,
  });

  return (
    <Card>
      <div className="px-5 py-5">
        <h2 className="m-0 mb-1 font-serif font-medium text-[22px] tracking-tight text-text">
          Start from a template
        </h2>
        <p className="m-0 mb-5 text-text-3 text-[13px] leading-snug">
          Templates seed the form — slot names, prompts, structure. You can
          rename anything and add or remove slots after.
        </p>

        {q.isPending ? (
          <div className="text-text-3 text-[13px]">Loading…</div>
        ) : q.isError ? (
          <div className="text-danger text-[13px]">
            Couldn't load templates. You can still start from a blank agent.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
            {(q.data ?? []).map((t) => (
              <TemplateCard key={t.id} template={t} onPick={() => onPick(t)} />
            ))}
          </div>
        )}

        <button
          type="button"
          onClick={onSkip}
          className="text-text-3 hover:text-text text-[13px] underline-offset-4 hover:underline transition-colors"
        >
          Start from blank instead
        </button>
      </div>
    </Card>
  );
}

function TemplateCard({
  template,
  onPick,
}: {
  template: AgentTemplate;
  onPick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPick}
      className="text-left bg-surface-panel border border-border rounded-card p-4 hover:border-gold/40 hover:bg-surface-hover transition-colors"
    >
      <div className="flex items-center gap-2 mb-1.5">
        <span className="text-[14px] font-medium text-text">
          {template.name}
        </span>
        <span className="text-[10px] text-text-3 font-mono uppercase tracking-wide">
          {template.slots.length} slot{template.slots.length === 1 ? "" : "s"}
        </span>
      </div>
      <p className="m-0 mb-3 text-text-2 text-[12.5px] leading-snug">
        {template.description}
      </p>
      <div className="flex flex-wrap gap-1.5">
        {template.slots.map((s) => (
          <span
            key={s.name}
            className="font-mono text-[10.5px] text-text-3 bg-bg/40 border border-border-soft rounded-sm px-1.5 py-0.5"
          >
            {s.name}
          </span>
        ))}
      </div>
    </button>
  );
}
