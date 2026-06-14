import { Suspense, type ElementType, lazy } from "react";
import { Outlet } from "react-router-dom";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";

const StripDockSlot = lazy(() =>
  import("@/features/agent-runs/StripDockSlot").then((m) => ({ default: m.StripDockSlot })),
);

export function TabletSplitShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  return (
    <div className="grid grid-cols-[min(360px,45vw)_minmax(0,1fr)] h-[100dvh] bg-bg text-text overflow-hidden">
      <div className="min-w-0 overflow-hidden">
        <Suspense fallback={null}>
          <ChatRailComponent
            variant="panel"
            className="min-w-0 border-r border-border-soft overflow-hidden"
          />
        </Suspense>
      </div>
      <main className="min-w-0 min-h-0 overflow-y-auto overflow-x-hidden px-6 pt-6 pb-6">
        <Outlet />
      </main>
      <CommandPalette />
      <Suspense fallback={null}>
        <StripDockSlot />
      </Suspense>
    </div>
  );
}
