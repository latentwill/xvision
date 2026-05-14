// Skills API — workspace-level registry of reusable modules agent slots
// can reference. See `crates/xvision-dashboard/src/routes/skills.rs`.

import { apiFetch } from "./client";

export type SkillKind = "tool" | "prompt_fragment" | "evaluator";

export type Skill = {
  skill_id: string;
  name: string;
  description: string;
  kind: SkillKind;
  config: Record<string, unknown>;
  archived: boolean;
  created_at: string;
  updated_at: string;
};

export type CreateSkillBody = {
  name: string;
  description?: string;
  kind: SkillKind;
  config?: Record<string, unknown>;
};

export type UpdateSkillBody = Partial<{
  name: string;
  description: string;
  kind: SkillKind;
  config: Record<string, unknown>;
}>;

export async function listSkills(
  includeArchived = false,
): Promise<Skill[]> {
  const path = includeArchived
    ? "/api/skills?include_archived=true"
    : "/api/skills";
  const res = await apiFetch<{ items: Skill[] }>(path);
  return res.items;
}

export async function createSkill(body: CreateSkillBody): Promise<Skill> {
  return apiFetch<Skill>("/api/skills", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function updateSkill(
  skillId: string,
  body: UpdateSkillBody,
): Promise<Skill> {
  return apiFetch<Skill>(`/api/skills/${encodeURIComponent(skillId)}`, {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function archiveSkill(skillId: string): Promise<void> {
  await apiFetch<{ archived: boolean }>(
    `/api/skills/${encodeURIComponent(skillId)}`,
    { method: "DELETE" },
  );
}

export const skillKeys = {
  all: ["skills"] as const,
  list: (includeArchived = false) =>
    [...skillKeys.all, "list", includeArchived] as const,
  detail: (id: string) => [...skillKeys.all, "detail", id] as const,
};
