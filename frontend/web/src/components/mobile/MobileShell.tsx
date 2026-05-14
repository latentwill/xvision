import { Outlet, useLocation } from "react-router-dom";

import { ChatRail } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";

export function MobileShell() {
  const location = useLocation();
  const isHome = location.pathname === "/";

  return (
    <div className="min-h-screen bg-bg text-text overflow-hidden">
      {isHome ? (
        <div className="h-screen">
          <ChatRail variant="panel" />
        </div>
      ) : (
        <main className="min-h-screen overflow-x-hidden px-4 pt-4 pb-6">
          <Outlet />
        </main>
      )}
      <CommandPalette />
    </div>
  );
}
