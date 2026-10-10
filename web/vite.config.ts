import { defineConfig } from 'vitest/config';

export default defineConfig({
  base: '/',
  // CodeMirror's Markdown mode pulls in HTML/CSS/JS parsers; fine for a local tool.
  build: { outDir: 'dist', emptyOutDir: true, chunkSizeWarningLimit: 1200 },
  test: { environment: 'jsdom' },
});
