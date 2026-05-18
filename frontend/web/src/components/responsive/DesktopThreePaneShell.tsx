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
    <div className="grid grid-cols-[220px_1fr_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="px-9 pt-9 pb-6 overflow-x-hidden">
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
