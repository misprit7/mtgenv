import { defineConfig } from "vite";

// Dev: `npm run dev` serves the front end with HMR and proxies the WebSocket to the running
// Rust server (`cargo run -p mtg-gre-server --bin mtg-serve`, default :8080).
// Prod: `npm run build` emits ./dist, which axum serves as static files (server.rs).
export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      "/ws": {
        target: "http://127.0.0.1:8080",
        ws: true,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
