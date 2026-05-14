import { MobileShell } from "@/components/mobile/MobileShell";
import { DesktopThreePaneShell } from "@/components/responsive/DesktopThreePaneShell";
import { TabletSplitShell } from "@/components/responsive/TabletSplitShell";
import { useViewportMode } from "@/components/responsive/useViewportMode";

export function Layout() {
  const mode = useViewportMode();

  if (mode === "phone") return <MobileShell />;
  if (mode === "tablet") return <TabletSplitShell />;
  return <DesktopThreePaneShell />;
}
