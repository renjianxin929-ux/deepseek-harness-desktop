import { resolve } from "node:path";
import { defineConfig } from "vite";

// The "frontend" is the startup/loading layer plus the appearance settings
// window. The DeepSeek Harness official Web UI is never bundled here — the
// WebView navigates to the locally started Harness server (127.0.0.1:<port>).
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        index: resolve(__dirname, "index.html"),
        appearance: resolve(__dirname, "appearance.html"),
      },
    },
  },
});
