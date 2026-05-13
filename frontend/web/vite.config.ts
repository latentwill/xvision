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
    // devices (e.g. a phone over Tailscale, or another machine on the LAN).
    // allowedHosts adds Tailscale MagicDNS names (*.ts.net) past Vite's
    // DNS-rebinding protection; loopback and private-IP access remain
    // implicitly allowed. See frontend/MOBILE.md §6 for the rationale and
    // frontend/README.md for the security caveat (no auth on the API).
    host: true,
    allowedHosts: [".ts.net"],
    proxy: {
      "/api": "http://127.0.0.1:8788",
    },
  },
});
