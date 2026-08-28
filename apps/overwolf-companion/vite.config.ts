import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1421,
    strictPort: true,
  },
  preview: {
    host: "127.0.0.1",
    port: 1421,
    strictPort: true,
  },
  build: {
    target: "es2020",
    modulePreload: {
      polyfill: false,
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
