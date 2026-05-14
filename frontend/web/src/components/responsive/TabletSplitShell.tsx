import { Outlet } from "react-router-dom";

import { ChatRail } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";

export function TabletSplitShell() {
  return (
    <div className="grid grid-cols-[360px_minmax(0,1fr)] h-screen bg-bg text-text overflow-hidden">
      <ChatRail
        variant="panel"
        className="min-w-0 border-r border-border-soft overflow-hidden"
      />
      <main className="min-w-0 min-h-0 overflow-y-auto overflow-x-hidden px-6 pt-6 pb-6">
        <Outlet />
      </main>
      <CommandPalette />
    </div>
  );
}
