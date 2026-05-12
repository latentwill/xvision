// Settings → Skills — registry CRUD for the workspace skill library.
// Agents reference skills by skill_id from this list.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import {
  archiveSkill,
  createSkill,
  listSkills,
  skillKeys,
  updateSkill,
  type Skill,
  type SkillKind,
} from "@/api/skills";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";

const KIND_OPTIONS: { value: SkillKind; label: string; blurb: string }[] = [
  {
    value: "tool",
    label: "Tool",
    blurb: "MCP-style callable an agent can invoke during a cycle.",
  },
  {
    value: "prompt_fragment",
    label: "Prompt fragment",
    blurb: "Text prepended to the agent's system prompt.",
  },
  {
    value: "evaluator",
    label: "Evaluator",
    blurb: "Post-decision check that can veto or annotate.",
  },
];

export function SettingsSkillsRoute() {
  const qc = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const q = useQuery({
    queryKey: skillKeys.list(false),
    queryFn: () => listSkills(false),
  });

  const archive = useMutation({
    mutationFn: archiveSkill,
    onSuccess: () => qc.invalidateQueries({ queryKey: skillKeys.all }),
  });

  return (
    <Card className="p-5">
      <div className="flex items-center justify-between mb-3">
        <div>
          <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
            Skills
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Reusable modules that agent slots can attach: tools, prompt
            fragments, evaluators. Add a skill here, then reference it from
            an agent's slot on the Agents page.
          </p>
        </div>
        {!adding ? (
          <button
            type="button"
            onClick={() => setAdding(true)}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
          >
            + Add skill
          </button>
        ) : null}
      </div>

      {adding ? (
        <SkillForm
          mode="create"
          onCancel={() => setAdding(false)}
          onDone={() => {
            setAdding(false);
            qc.invalidateQueries({ queryKey: skillKeys.all });
          }}
        />
      ) : null}

      {q.isPending ? (
        <div className="text-text-3 text-[13px] py-6 text-center">Loading…</div>
      ) : q.isError ? (
        <div className="text-danger text-[13px] py-6 text-center">
          {errorMessage(q.error)}
        </div>
      ) : (q.data ?? []).length === 0 && !adding ? (
        <div className="text-text-3 text-[13px] py-6 text-center">
          No skills yet — click <span className="text-text">+ Add skill</span> to
          create one.
        </div>
      ) : (
        <table className="w-full mt-2">
          <thead>
            <tr className="text-text-3 text-[11px] uppercase tracking-wider text-left">
              <th className="py-2 pr-3 font-normal">Name</th>
              <th className="py-2 pr-3 font-normal">Kind</th>
              <th className="py-2 pr-3 font-normal">Description</th>
              <th className="py-2 pr-0 font-normal text-right" />
            </tr>
          </thead>
          <tbody>
            {(q.data ?? []).map((skill) => (
              <SkillRow
                key={skill.skill_id}
                skill={skill}
                editing={editingId === skill.skill_id}
                onEdit={() => setEditingId(skill.skill_id)}
                onCancelEdit={() => setEditingId(null)}
                onSaved={() => {
                  setEditingId(null);
                  qc.invalidateQueries({ queryKey: skillKeys.all });
                }}
                onArchive={() => archive.mutate(skill.skill_id)}
                archiving={
                  archive.variables === skill.skill_id && archive.isPending
                }
              />
            ))}
          </tbody>
        </table>
      )}
    </Card>
  );
}

function SkillRow({
  skill,
  editing,
  onEdit,
  onCancelEdit,
  onSaved,
  onArchive,
  archiving,
}: {
  skill: Skill;
  editing: boolean;
  onEdit: () => void;
  onCancelEdit: () => void;
  onSaved: () => void;
  onArchive: () => void;
  archiving: boolean;
}) {
  if (editing) {
    return (
      <tr>
        <td colSpan={4} className="py-3">
          <SkillForm
            mode="edit"
            skill={skill}
            onCancel={onCancelEdit}
            onDone={onSaved}
          />
        </td>
      </tr>
    );
  }

  return (
    <tr className="border-t border-border-soft align-middle">
      <td className="py-2 pr-3">
        <code className="font-mono text-[13px] text-text">{skill.name}</code>
      </td>
      <td className="py-2 pr-3">
        <Pill tone={skill.kind === "tool" ? "gold" : "default"}>
          {skill.kind.replace("_", " ")}
        </Pill>
      </td>
      <td className="py-2 pr-3 text-text-2 text-[13px]">
        {skill.description || (
          <span className="text-text-3 italic text-[12px]">no description</span>
        )}
      </td>
      <td className="py-2 pr-0 text-right">
        <button
          type="button"
          onClick={onEdit}
          className="text-[12px] text-text-3 hover:text-text mr-3"
        >
          Edit
        </button>
        <button
          type="button"
          onClick={onArchive}
          disabled={archiving}
          className="text-[12px] text-text-3 hover:text-danger disabled:opacity-50"
        >
          {archiving ? "…" : "Archive"}
        </button>
      </td>
    </tr>
  );
}

function SkillForm({
  mode,
  skill,
  onCancel,
  onDone,
}: {
  mode: "create" | "edit";
  skill?: Skill;
  onCancel: () => void;
  onDone: () => void;
}) {
  const [name, setName] = useState(skill?.name ?? "");
  const [description, setDescription] = useState(skill?.description ?? "");
  const [kind, setKind] = useState<SkillKind>(skill?.kind ?? "tool");
  const [configText, setConfigText] = useState(
    JSON.stringify(skill?.config ?? {}, null, 2),
  );
  const [error, setError] = useState<string | null>(null);

  const m = useMutation({
    mutationFn: async () => {
      let parsedConfig: Record<string, unknown>;
      try {
        parsedConfig = JSON.parse(configText) as Record<string, unknown>;
      } catch (e) {
        throw new Error(`Config must be valid JSON: ${(e as Error).message}`);
      }

      if (mode === "create") {
        return createSkill({
          name,
          description,
          kind,
          config: parsedConfig,
        });
      } else {
        return updateSkill(skill!.skill_id, {
          name,
          description,
          kind,
          config: parsedConfig,
        });
      }
    },
    onSuccess: () => onDone(),
    onError: (e) => setError(errorMessage(e)),
  });

  function onSubmit() {
    setError(null);
    if (!name.trim()) {
      setError("Name is required");
      return;
    }
    m.mutate();
  }

  return (
    <div className="bg-surface-panel border border-border rounded-card p-4 mb-3">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3 mb-3">
        <Field label="Name">
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. rsi-tool"
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
        </Field>

        <Field label="Kind">
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value as SkillKind)}
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
          >
            {KIND_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <p className="m-0 mt-1 text-text-3 text-[11px] leading-snug">
            {KIND_OPTIONS.find((o) => o.value === kind)?.blurb}
          </p>
        </Field>
      </div>

      <Field label="Description">
        <input
          type="text"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="One-line summary"
          className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
        />
      </Field>

      <div className="mt-3">
        <Field label="Config (JSON)">
          <textarea
            value={configText}
            onChange={(e) => setConfigText(e.target.value)}
            rows={4}
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[12.5px] text-text font-mono leading-relaxed focus:outline-none focus:border-gold/40 resize-y"
          />
        </Field>
      </div>

      {error ? (
        <div className="mt-3 text-danger text-[12.5px]">{error}</div>
      ) : null}

      <div className="flex items-center justify-end gap-2 mt-3">
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-1.5 rounded text-[12px] text-text-3 hover:text-text"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={onSubmit}
          disabled={m.isPending}
          className="px-3 py-1.5 rounded text-[12px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-50"
        >
          {m.isPending ? "Saving…" : mode === "create" ? "Create skill" : "Save"}
        </button>
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
        {label}
      </span>
      {children}
    </label>
  );
}

function errorMessage(e: unknown): string {
  if (e instanceof ApiError) return `${e.code}: ${e.message}`;
  if (e instanceof Error) return e.message;
  return String(e);
}
