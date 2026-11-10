import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Port fixe attendu par Tauri (voir src-tauri/tauri.conf.json).
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "es2022" },
});
