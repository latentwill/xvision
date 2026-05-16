import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Vite emits the SPA into ../../crates/xvision-dashboard/static so rust-embed
// can bake assets into the dashboard binary. Hot dev runs on its own port and
// proxies /api to the axum server.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  build: {
    outDir: path.resolve(
      __dirname,
      "../../crates/xvision-dashboard/static",
    ),
    emptyOutDir: true,
    sourcemap: false,
  },
  server: {
    port: 5180,
    strictPort: true,
    // host: true binds 0.0.0.0 so the dev server is reachable from other
    // devices (a phone over Tailscale, another machine on the LAN, or via
    // Bonjour .local). allowedHosts: true disables Vite 5's DNS-rebinding
    // protection so any hostname resolves — needed because iPhone Safari
    // on the same Wi-Fi hits the Mac via Bonjour (e.g. Eds-MacBook-Pro.local),
    // which Vite would otherwise reject with a "Blocked request" 403 and
    // surface to the user as "doesn't load." The dashboard has no auth
    // either way (DESIGN.md §8.4); the actual trust boundary is the network
    // (tailnet ACL or trusted LAN), not the dev-server allowlist. Do not
    // run `pnpm dev` on an untrusted shared network. See frontend/MOBILE.md
    // §6 and frontend/README.md for the security caveat, and FOLLOWUPS F35
    // for the API-auth track that gates wider-bind production deployments.
    host: true,
    allowedHosts: true,
    proxy: {
      "/api": "http://127.0.0.1:8788",
    },
  },
});
