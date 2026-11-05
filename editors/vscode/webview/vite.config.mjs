import { fileURLToPath } from 'node:url'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

const here = (path) => fileURLToPath(new URL(path, import.meta.url))

// 輸出固定檔名，插件以 asWebviewUri 載入 index.js 與 index.css
export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: here('../out/webview'),
    emptyOutDir: true,
    lib: {
      entry: here('main.ts'),
      formats: ['iife'],
      name: 'kifedit',
      fileName: () => 'index.js',
      cssFileName: 'index',
    },
  },
})
