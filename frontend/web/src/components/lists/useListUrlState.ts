import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router-dom";

import type { ListState } from "./useListState";

export function useListUrlState(listId: string, state: ListState<unknown>): void {
  const [params, setParams] = useSearchParams();
  const hydratedRef = useRef(false);
  const stateRef = useRef(state);
  stateRef.current = state;

  useEffect(() => {
    if (hydratedRef.current) return;
    hydratedRef.current = true;
    const s = stateRef.current;

    const q = params.get("q");
    if (q && q !== s.search.value) s.search.setValue(q);

    s.filters.forEach((f) => {
      const v = params.get(f.def.id);
      if (v && v !== f.value) f.setValue(v);
    });

    const sort = params.get("sort");
    if (sort && sort !== s.sort.value) s.sort.setValue(sort);
  }, [params]);

  useEffect(() => {
    if (!hydratedRef.current) return;
    const next = new URLSearchParams(params);
    const setOrDelete = (key: string, value: string, isDefault: boolean) => {
      if (!value || isDefault) next.delete(key);
      else next.set(key, value);
    };

    setOrDelete("q", state.search.value, state.search.value === "");

    state.filters.forEach((f) => {
      const defaultValue =
        f.def.defaultValue ?? f.def.options[0]?.value ?? "";
      setOrDelete(f.def.id, f.value, f.value === defaultValue);
    });

    const defaultSort = state.sort.options[0]?.value ?? "";
    setOrDelete("sort", state.sort.value, state.sort.value === defaultSort);

    if (next.toString() !== params.toString()) {
      setParams(next, { replace: true });
    }
    // listId is part of the dep list so distinct hosts don't accidentally
    // share URL writes; the value isn't read otherwise.
  }, [
    listId,
    state.search.value,
    state.filters.map((f) => `${f.def.id}=${f.value}`).join("&"),
    state.sort.value,
  ]);
}
