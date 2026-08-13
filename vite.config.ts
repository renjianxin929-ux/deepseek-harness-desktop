import { defineConfig } from "vite";

// Minimal Vite config: the "frontend" is only the startup/loading layer.
// The DeepSeek Harness official Web UI is never bundled here — the WebView
// navigates to the locally started Harness server (127.0.0.1:<dynamic-port>).
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
  },
});
