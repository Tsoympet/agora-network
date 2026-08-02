import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@brand": path.resolve(__dirname, "../shared/brand"),
    },
  },
  server: {
    port: 5173,
  },
});
