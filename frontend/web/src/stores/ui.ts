import { create } from "zustand";

// Ephemeral UI state. Server state lives in TanStack Query; long-lived prefs
// (theme, last-route, env selection) belong here once they exist.

type UiState = {
  chatRailOpen: boolean;
  setChatRailOpen: (open: boolean) => void;
  toggleChatRail: () => void;
};

export const useUi = create<UiState>((set) => ({
  chatRailOpen: false,
  setChatRailOpen: (open) => set({ chatRailOpen: open }),
  toggleChatRail: () => set((s) => ({ chatRailOpen: !s.chatRailOpen })),
}));
