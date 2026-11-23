import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Built into dist/, which the Rust side embeds (tauri.conf.json: frontendDist).
// `make dev` runs the dev server instead; the app loads it from devUrl.
// 127.0.0.1, not localhost: the Windows app reaches it through WSL's localhost forwarding.
export default defineConfig({
  plugins: [react()],
  base: "./",
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
  server: { host: "127.0.0.1", port: 5173, strictPort: true },
});
