import { Suspense, type ElementType, lazy } from "react";
import { Outlet } from "react-router-dom";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Sidebar } from "@/components/shell/Sidebar";
import { useFirstRunTour } from "@/features/onboarding";

const StripDockSlot = lazy(() =>
  import("@/features/agent-runs/StripDockSlot").then((m) => ({ default: m.StripDockSlot })),
);

export function DesktopThreePaneShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  useFirstRunTour();
  return (
    <div className="grid grid-cols-[220px_minmax(0,1fr)_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="min-w-0 max-w-[960px] w-full justify-self-center px-6 pt-7 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      <Suspense fallback={null}>
        <ChatRailComponent />
      </Suspense>
      <CommandPalette />
      <Suspense fallback={null}>
        <StripDockSlot />
      </Suspense>
    </div>
  );
}
