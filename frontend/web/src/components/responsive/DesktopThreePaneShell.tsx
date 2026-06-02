import { Suspense, type ElementType, lazy } from "react";
import { Outlet } from "react-router-dom";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Sidebar } from "@/components/shell/Sidebar";
import { useFirstRunTour } from "@/features/onboarding";
import { useUi } from "@/stores/ui";
import { ResizeHandle } from "./ResizeHandle";

const StripDockSlot = lazy(() =>
  import("@/features/agent-runs/StripDockSlot").then((m) => ({ default: m.StripDockSlot })),
);

export function DesktopThreePaneShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  useFirstRunTour();

  const sidebarWidth = useUi((s) => s.sidebarWidth);
  const chatRailOpen = useUi((s) => s.chatRailOpen);
  const setSidebarWidth = useUi((s) => s.setSidebarWidth);
  const setChatRailWidth = useUi((s) => s.setChatRailWidth);
  const chatRailWidth = useUi((s) => s.chatRailWidth);

  // Always 5 columns: sidebar | left-handle | center | right-handle | rail.
  // When rail is closed the right handle is hidden (0px) and rail is auto
  // (collapsed icon strip). Keeping 5 columns avoids implicit row creation.
  const gridTemplateColumns = chatRailOpen
    ? `${sidebarWidth}px 4px minmax(0,1fr) 4px ${chatRailWidth}px`
    : `${sidebarWidth}px 4px minmax(0,1fr) 0px auto`;

  return (
    <div
      className="grid min-h-screen bg-bg text-text"
      style={{ gridTemplateColumns }}
    >
      <Sidebar />
      <ResizeHandle onDelta={(d) => setSidebarWidth(sidebarWidth + d)} />
      <main className="min-w-0 max-w-[960px] w-full justify-self-center px-6 pt-7 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      {/* Always render right handle to keep 5-column grid stable; hidden when rail closed */}
      <ResizeHandle
        onDelta={(d) => setChatRailWidth(chatRailWidth - d)}
        hidden={!chatRailOpen}
      />
      <div className="min-w-[44px]">
        <Suspense fallback={<div className="w-[44px]" />}>
          <ChatRailComponent />
        </Suspense>
      </div>
      <CommandPalette />
      <Suspense fallback={null}>
        <StripDockSlot />
      </Suspense>
    </div>
  );
}
