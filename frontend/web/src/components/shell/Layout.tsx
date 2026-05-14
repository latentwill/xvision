import { lazy } from "react";

import { MobileShell } from "@/components/mobile/MobileShell";
import { DesktopThreePaneShell } from "@/components/responsive/DesktopThreePaneShell";
import { TabletSplitShell } from "@/components/responsive/TabletSplitShell";
import { useViewportMode } from "@/components/responsive/useViewportMode";
import { TourController } from "@/tours/TourController";

const LazyChatRail = lazy(() =>
  import("./ChatRail").then((m) => ({ default: m.ChatRail })),
);

export function Layout() {
  const mode = useViewportMode();
  const shell =
    mode === "phone" ? (
      <MobileShell ChatRailComponent={LazyChatRail} />
    ) : mode === "tablet" ? (
      <TabletSplitShell ChatRailComponent={LazyChatRail} />
    ) : (
      <DesktopThreePaneShell ChatRailComponent={LazyChatRail} />
    );

  return (
    <>
      {shell}
      <TourController />
    </>
  );
}
