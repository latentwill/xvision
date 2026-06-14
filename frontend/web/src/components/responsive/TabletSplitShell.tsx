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
  // QA #5: keep the chat rail on the RIGHT edge at tablet width (main first,
  // rail last) so it matches the desktop three-pane shell and never flips to
  // the left column as the viewport narrows.
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_min(360px,45vw)] h-[100dvh] bg-bg text-text overflow-hidden">
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
