import { defineConfig } from "vite";
export default defineConfig({
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/artifacts/**"] },
  },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_"],
});
