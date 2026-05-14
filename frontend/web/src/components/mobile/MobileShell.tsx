import { Outlet, useLocation } from "react-router-dom";

import { ChatRail } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { MobileDrawer } from "@/components/mobile/MobileDrawer";
import { MobileFunctionsSheet } from "@/components/mobile/MobileFunctionsSheet";
import { MobileTopBar } from "@/components/mobile/MobileTopBar";
import { useUi } from "@/stores/ui";

export function MobileShell() {
  const location = useLocation();
  const setDrawerOpen = useUi((s) => s.setMobileDrawerOpen);
  const setFunctionsOpen = useUi((s) => s.setMobileFunctionsOpen);
  const isHome = location.pathname === "/";
  const title = routeTitle(location.pathname);

  return (
    <div className="h-screen bg-bg text-text overflow-hidden flex flex-col">
      <MobileTopBar
        title={isHome ? undefined : title}
        onMenu={() => setDrawerOpen(true)}
      />
      {isHome ? (
        <div className="flex-1 min-h-0">
          <ChatRail
            variant="panel"
            showHeader={false}
            onOpenActions={() => setFunctionsOpen(true)}
          />
        </div>
      ) : (
        <main className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden px-4 pt-4 pb-6">
          <Outlet />
        </main>
      )}
      <MobileDrawer />
      <MobileFunctionsSheet />
      <CommandPalette />
    </div>
  );
}

function routeTitle(pathname: string): string {
  if (pathname.startsWith("/eval-runs/")) return "Eval run";
  if (pathname === "/eval-runs") return "Eval runs";
  if (pathname.startsWith("/strategies")) return "Strategies";
  if (pathname.startsWith("/agents")) return "Agents";
  if (pathname.startsWith("/authoring")) return "Authoring";
  if (pathname.startsWith("/settings")) return "Settings";
  if (pathname.startsWith("/setup")) return "Setup";
  return "xvn";
}
