import { Outlet } from "react-router-dom";

import { ChatRail } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Sidebar } from "@/components/shell/Sidebar";

export function DesktopThreePaneShell() {
  return (
    <div className="grid grid-cols-[220px_1fr_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="px-9 pt-9 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      <ChatRail />
      <CommandPalette />
    </div>
  );
}
