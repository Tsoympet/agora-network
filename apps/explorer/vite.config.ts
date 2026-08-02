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
    proxy: {
      "/rpc": {
        target: process.env.AGORA_RPC_PROXY || "http://127.0.0.1:8545",
        changeOrigin: true,
      },
      "/health": {
        target: process.env.AGORA_RPC_PROXY || "http://127.0.0.1:8545",
        changeOrigin: true,
      },
    },
  },
});
