import { create } from "zustand";

// Ephemeral UI state. Server state lives in TanStack Query; long-lived prefs
// (theme, last-route, env selection) belong here once they exist.

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
};

export const useUi = create<UiState>((set) => ({
  chatRailOpen: false,
  setChatRailOpen: (open) => set({ chatRailOpen: open }),
  toggleChatRail: () => set((s) => ({ chatRailOpen: !s.chatRailOpen })),
  cmdkOpen: false,
  setCmdkOpen: (open) => set({ cmdkOpen: open }),
  toggleCmdk: () => set((s) => ({ cmdkOpen: !s.cmdkOpen })),
  mobileDrawerOpen: false,
  setMobileDrawerOpen: (open) => set({ mobileDrawerOpen: open }),
  mobileFunctionsOpen: false,
  setMobileFunctionsOpen: (open) => set({ mobileFunctionsOpen: open }),
}));
