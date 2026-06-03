import { create } from "zustand";

import { safeStorageGet, safeStorageSet } from "@/lib/storage";

const SIDEBAR_WIDTH_LS = "xvn.sidebar.width";
const RAIL_WIDTH_LS = "xvn.rail.width";
const RAIL_OPEN_LS = "xvn.chat_rail.open";

const SIDEBAR_MIN = 160;
const SIDEBAR_MAX = 360;
const SIDEBAR_DEFAULT = 220;
const RAIL_MIN = 280;
const RAIL_MAX = 600;
const RAIL_DEFAULT = 380;

function clamp(v: number, min: number, max: number): number {
  return Math.min(Math.max(v, min), max);
}

function readInt(key: string, fallback: number): number {
  const raw = safeStorageGet(key);
  if (!raw) return fallback;
  const n = parseInt(raw, 10);
  return Number.isFinite(n) ? n : fallback;
}

type UiState = {
  chatRailOpen: boolean;
  setChatRailOpen: (open: boolean) => void;
  toggleChatRail: () => void;
  cmdkOpen: boolean;
  setCmdkOpen: (open: boolean) => void;
  toggleCmdk: () => void;
  mobileDrawerOpen: boolean;
  setMobileDrawerOpen: (open: boolean) => void;
  mobileFunctionsOpen: boolean;
  setMobileFunctionsOpen: (open: boolean) => void;
  sidebarWidth: number;
  setSidebarWidth: (w: number) => void;
  chatRailWidth: number;
  setChatRailWidth: (w: number) => void;
};

export const useUi = create<UiState>((set) => ({
  chatRailOpen: safeStorageGet(RAIL_OPEN_LS) === "1",
  setChatRailOpen: (open) => {
    safeStorageSet(RAIL_OPEN_LS, open ? "1" : "0");
    set({ chatRailOpen: open });
  },
  toggleChatRail: () =>
    set((s) => {
      const next = !s.chatRailOpen;
      safeStorageSet(RAIL_OPEN_LS, next ? "1" : "0");
      return { chatRailOpen: next };
    }),
  cmdkOpen: false,
  setCmdkOpen: (open) => set({ cmdkOpen: open }),
  toggleCmdk: () => set((s) => ({ cmdkOpen: !s.cmdkOpen })),
  mobileDrawerOpen: false,
  setMobileDrawerOpen: (open) => set({ mobileDrawerOpen: open }),
  mobileFunctionsOpen: false,
  setMobileFunctionsOpen: (open) => set({ mobileFunctionsOpen: open }),
  sidebarWidth: clamp(readInt(SIDEBAR_WIDTH_LS, SIDEBAR_DEFAULT), SIDEBAR_MIN, SIDEBAR_MAX),
  setSidebarWidth: (w) => {
    const clamped = clamp(w, SIDEBAR_MIN, SIDEBAR_MAX);
    safeStorageSet(SIDEBAR_WIDTH_LS, String(clamped));
    set({ sidebarWidth: clamped });
  },
  chatRailWidth: clamp(readInt(RAIL_WIDTH_LS, RAIL_DEFAULT), RAIL_MIN, RAIL_MAX),
  setChatRailWidth: (w) => {
    const clamped = clamp(w, RAIL_MIN, RAIL_MAX);
    safeStorageSet(RAIL_WIDTH_LS, String(clamped));
    set({ chatRailWidth: clamped });
  },
}));
