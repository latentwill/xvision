import { lazy, Suspense } from "react";
import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { CommandPalette } from "./CommandPalette";

const ChatRail = lazy(() =>
  import("./ChatRail").then((m) => ({ default: m.ChatRail })),
);

export function Layout() {
  return (
    <div className="grid grid-cols-[200px_1fr_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="px-9 pt-9 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      <Suspense fallback={<div className="w-11" />}>
        <ChatRail />
      </Suspense>
      <CommandPalette />
    </div>
  );
}
