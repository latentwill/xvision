import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Vite emits the SPA into ../../crates/xvision-dashboard/static so rust-embed
// can bake assets into the dashboard binary. Hot dev runs on its own port and
// proxies /api to the axum server.

// Default dev allowlist: Tailscale MagicDNS + Bonjour mDNS. Vite 5 already
// implicitly accepts loopback, IPv4 literals, and *.localhost; the entries
// here cover the two name-based paths a phone actually uses to reach the
// Mac (Tailscale `*.ts.net`, and Wi-Fi Bonjour `*.local` such as
// `Eds-MacBook-Pro.local`). Keep the list explicit so Vite's DNS-rebinding
// protection still rejects unknown hostnames. For one-off names (a custom
// hosts entry, a CI runner DNS), extend at launch:
//   XVN_DEV_ALLOWED_HOSTS=foo.test,bar.internal pnpm dev
const baseAllowedHosts = [".ts.net", ".local"];
const extraAllowedHosts = (process.env.XVN_DEV_ALLOWED_HOSTS ?? "")
  .split(",")
  .map((entry) => entry.trim())
  .filter(Boolean);

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
    // devices. See frontend/MOBILE.md §6 and frontend/README.md for the
    // security caveat (no auth on /api — DESIGN.md §8.4). The trust boundary
    // is the network (tailnet ACL or trusted LAN); do not run `pnpm dev` on
    // an untrusted shared network. FOLLOWUPS F35 tracks the API-auth work
    // that gates any wider-bind production deployment.
    host: true,
    allowedHosts: [...baseAllowedHosts, ...extraAllowedHosts],
    proxy: {
      "/api": "http://127.0.0.1:8788",
    },
  },
});
