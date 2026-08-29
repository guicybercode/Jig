import react from "@vitejs/plugin-react";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    clearMocks: true,
    environment: "jsdom",
    exclude: [...configDefaults.exclude, "**/e2e/**"],
    restoreMocks: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
