import { defineConfig } from "vite";

export default defineConfig({
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2023",
  },
  server: {
    host: "127.0.0.1",
  },
});
