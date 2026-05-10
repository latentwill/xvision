import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { ChatRail } from "./ChatRail";

export function Layout() {
  return (
    <div className="grid grid-cols-[200px_1fr_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="px-9 pt-9 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      <ChatRail />
    </div>
  );
}
