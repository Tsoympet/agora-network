import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      "@agora/light-client": path.resolve(__dirname, "../shared/light-client"),
    },
  },
  server: {
    port: 5174,
    strictPort: true,
    proxy: {
      "/rpc": {
        target: process.env.AGORA_RPC_PROXY || "http://127.0.0.1:8545",
        changeOrigin: true,
      },
    },
  },
  publicDir: "public",
});
