// frontend/web/src/stores/trace-dock.ts
import { create } from "zustand";

export type DockHeight = "collapsed" | "peek" | "working" | "full";
export type DockMode = "post-hoc" | "live";

type State = {
  height: DockHeight;
  selectedSpanId: string | null;
  activeRunId: string | null;
  mode: DockMode;
  /** Last non-collapsed height — restored by toggle(). */
  lastOpenHeight: DockHeight;
};

type Actions = {
  setHeight: (h: DockHeight) => void;
  toggle: () => void;
  minimize: () => void;
  setSelectedSpan: (id: string | null) => void;
  setActiveRun: (id: string | null, mode: DockMode) => void;
};

export const useTraceDock = create<State & Actions>((set, get) => ({
  height: "collapsed",
  selectedSpanId: null,
  activeRunId: null,
  mode: "post-hoc",
  lastOpenHeight: "working",
  setHeight: (h) =>
    set((s) => ({
      height: h,
      lastOpenHeight: h === "collapsed" ? s.lastOpenHeight : h,
    })),
  toggle: () => {
    const s = get();
    set({
      height: s.height === "collapsed" ? s.lastOpenHeight : "collapsed",
    });
  },
  minimize: () => set({ height: "collapsed" }),
  setSelectedSpan: (id) => set({ selectedSpanId: id }),
  setActiveRun: (id, mode) =>
    set({ activeRunId: id, mode, selectedSpanId: null }),
}));
