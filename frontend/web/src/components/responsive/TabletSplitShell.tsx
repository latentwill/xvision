import { Suspense, type ElementType, lazy } from "react";
import { Outlet } from "react-router-dom";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Sidebar } from "@/components/shell/Sidebar";

const StripDockSlot = lazy(() =>
  import("@/features/agent-runs/StripDockSlot").then((m) => ({ default: m.StripDockSlot })),
);

export function TabletSplitShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  // QA: the tablet shell (768–1279px) previously rendered no left nav at all —
  // the side menu "disappeared" as the viewport narrowed out of the desktop
  // shell. Render the icon-rail (compact) Sidebar so nav stays reachable, while
  // QA #5's chat rail stays pinned to the RIGHT edge (sidebar | main | rail).
  return (
    <div className="grid grid-cols-[60px_minmax(0,1fr)_min(320px,40vw)] h-[100dvh] bg-bg text-text overflow-hidden">
      <Sidebar compact />
      <main className="min-w-0 min-h-0 overflow-y-auto overflow-x-hidden px-6 pt-6 pb-6">
        <Outlet />
      </main>
      <div className="min-w-0 overflow-hidden">
        <Suspense fallback={null}>
          <ChatRailComponent
            variant="panel"
            className="min-w-0 border-l border-border-soft overflow-hidden"
          />
        </Suspense>
      </div>
      <CommandPalette />
      <Suspense fallback={null}>
        <StripDockSlot />
      </Suspense>
    </div>
  );
}
