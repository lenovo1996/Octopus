import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/ — plain Vite SPA (no SvelteKit) per docs/03-architecture.md
export default defineConfig(() => ({
  plugins: [svelte()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"]
    }
  },
  // 4. tauri `frontendDist` (src-tauri/tauri.conf.json) points at `dist`
  build: {
    outDir: "dist"
  }
}));
