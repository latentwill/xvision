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
    sourcemap: true,
  },
  server: {
    port: 5180,
    strictPort: true,
    proxy: {
      "/api": "http://127.0.0.1:8788",
    },
  },
});
