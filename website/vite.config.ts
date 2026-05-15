import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import pkg from "./package.json" with { type: "json" };

export default defineConfig({
  define: {
    // Build-time constants. CI injects VITE_VERSION / VITE_GIT_COMMIT /
    // VITE_BUILD_TIME from `github.sha` + `github.run_id`.
    __CLIENT_VERSION__: JSON.stringify(process.env.VITE_VERSION || pkg.version),
    __BUILD_TIME__: JSON.stringify(process.env.VITE_BUILD_TIME || new Date().toISOString()),
    __GIT_COMMIT__: JSON.stringify(process.env.VITE_GIT_COMMIT || "unknown"),
    __GIT_BRANCH__: JSON.stringify(process.env.VITE_GIT_BRANCH || "unknown"),
    // Defaults to `/api/v1` so the dev server proxies through to the Rust
    // backend on :8000. Override at build time for split deployments.
    __API_URL__: JSON.stringify(process.env.VITE_API_URL || "/api/v1"),
    // Telegram Login Widget needs the bot username to render the auth button.
    // The runtime can still override via `window.__BOT_USERNAME__` for ops.
    __BOT_USERNAME__: JSON.stringify(process.env.VITE_BOT_USERNAME || ""),
  },
  plugins: [tailwindcss(), solid()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 3000,
    proxy: {
      "/api": {
        target: "http://localhost:8000",
        changeOrigin: true,
      },
    },
  },
  preview: {
    port: 4173,
    proxy: {
      "/api": {
        target: "http://localhost:8000",
        changeOrigin: true,
      },
    },
  },
  build: {
    target: "esnext",
    sourcemap: false,
  },
});
