import { apiFetch } from "./client";

export type ToolCatalogEntry = {
  name: string;
  description: string;
  input_schema: unknown;
  built_in: boolean;
};

export type ToolsListResponse = {
  items: ToolCatalogEntry[];
};

export async function listTools(): Promise<ToolsListResponse> {
  return apiFetch<ToolsListResponse>("/api/tools");
}

export const toolKeys = {
  all: ["tools"] as const,
};
