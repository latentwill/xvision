import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";

export const MIN_COMPARE_IDS = 2;
export const MAX_COMPARE_IDS = 10;

export type CompareSelection = {
  selectedIds: string[];
  count: number;
  add: (id: string) => void;
  remove: (id: string) => void;
  toggle: (id: string) => void;
  setLead: (id: string) => void;
};

export function parseCompareIds(raw: string | null): string[] {
  if (!raw) return [];
  const seen = new Set<string>();
  const ids: string[] = [];
  for (const part of raw.split(",")) {
    const id = part.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    ids.push(id);
    if (ids.length >= MAX_COMPARE_IDS) break;
  }
  return ids;
}

export function useCompareSelection(): CompareSelection {
  const [params, setParams] = useSearchParams();
  const selectedIds = useMemo(
    () => parseCompareIds(params.get("ids")),
    [params],
  );

  const writeIds = (ids: string[]) => {
    const next = new URLSearchParams(params);
    if (ids.length > 0) {
      next.set("ids", ids.slice(0, MAX_COMPARE_IDS).join(","));
    } else {
      next.delete("ids");
    }
    setParams(next, { replace: false });
  };

  return {
    selectedIds,
    count: selectedIds.length,
    add: (rawId: string) => {
      const id = rawId.trim();
      if (!id || selectedIds.includes(id) || selectedIds.length >= MAX_COMPARE_IDS) return;
      writeIds([...selectedIds, id]);
    },
    remove: (id: string) => {
      if (selectedIds.length <= MIN_COMPARE_IDS) return;
      writeIds(selectedIds.filter((x) => x !== id));
    },
    toggle: (id: string) => {
      if (selectedIds.includes(id)) {
        if (selectedIds.length > MIN_COMPARE_IDS) {
          writeIds(selectedIds.filter((x) => x !== id));
        }
      } else if (selectedIds.length < MAX_COMPARE_IDS) {
        writeIds([...selectedIds, id]);
      }
    },
    setLead: (id: string) => {
      if (!selectedIds.includes(id) || selectedIds[0] === id) return;
      writeIds([id, ...selectedIds.filter((x) => x !== id)]);
    },
  };
}
