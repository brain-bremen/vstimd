import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In dev, proxy the WebSocket endpoints to the running vstimd server so the app
// can use same-origin URLs (ws://<host>/ws, /events). In production the React
// bundle is embedded in and served by the server, so same-origin already holds.
const SERVER = process.env.VSTIMD_WEB ?? "http://localhost:8080";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/ws": { target: SERVER, ws: true, changeOrigin: true },
      "/events": { target: SERVER, ws: true, changeOrigin: true },
    },
  },
  build: { outDir: "dist" },
});
