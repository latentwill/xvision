import { lazy } from "react";

import { MobileShell } from "@/components/mobile/MobileShell";
import { DesktopThreePaneShell } from "@/components/responsive/DesktopThreePaneShell";
import { TabletSplitShell } from "@/components/responsive/TabletSplitShell";
import { useViewportMode } from "@/components/responsive/useViewportMode";

const LazyChatRail = lazy(() =>
  import("./ChatRail").then((m) => ({ default: m.ChatRail })),
);

export function Layout() {
  const mode = useViewportMode();

  if (mode === "phone") return <MobileShell ChatRailComponent={LazyChatRail} />;
  if (mode === "tablet") {
    return <TabletSplitShell ChatRailComponent={LazyChatRail} />;
  }
  return <DesktopThreePaneShell ChatRailComponent={LazyChatRail} />;
}
