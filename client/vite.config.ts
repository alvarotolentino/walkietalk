import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

const port = parseInt(process.env.VITE_PORT || "1420");

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
