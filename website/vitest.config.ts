import { resolve } from "node:path";
import solid from "vite-plugin-solid";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
    },
    conditions: ["development", "browser"],
  },
  define: {
    __CLIENT_VERSION__: JSON.stringify("test"),
    __BUILD_TIME__: JSON.stringify("test"),
    __GIT_COMMIT__: JSON.stringify("test"),
    __GIT_BRANCH__: JSON.stringify("test"),
    __API_URL__: JSON.stringify("/api/v1"),
    __BOT_USERNAME__: JSON.stringify("vixen_test_bot"),
  },
  test: {
    environment: "jsdom",
    globals: false,
    setupFiles: ["./src/test-setup.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html", "lcov", "json-summary"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.test.{ts,tsx}",
        "src/test-setup.ts",
        "src/index.tsx",
        "src/pages/**",
        "src/shared/i18n/generated/**",
      ],
    },
  },
});
