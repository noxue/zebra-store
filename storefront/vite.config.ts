import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import vueJsx from '@vitejs/plugin-vue-jsx'
import tailwindcss from '@tailwindcss/vite'

const target = process.env.VITE_API_TARGET || 'http://localhost:8080'
const proxied = { target, changeOrigin: false }

export default defineConfig({
  plugins: [vueJsx(), tailwindcss()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    host: '0.0.0.0',
    port: 5185,
    strictPort: true,
    proxy: {
      '/api': proxied,
      '/uploads': proxied,
      '/sitemap.xml': proxied,
      '/robots.txt': proxied,
      // Provider-compat protocols called by acg-faka / mcy-shop downstream sites on the
      // storefront origin (the site address shown in 个人中心 → API 对接).
      '^/shared/': proxied,
      '^/plugin/open-api/': proxied,
    },
  },
  build: {
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        manualChunks: {
          'vendor-qrcode': ['qrcode'],
          'vendor-vue': ['vue', 'vue-router', 'pinia', 'vue-i18n'],
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    // Lets tests prove that injected <script> elements execute (custom scripts, QA-A08).
    environmentOptions: { jsdom: { runScripts: 'dangerously' } },
    include: ['tests/**/*.test.ts'],
  },
})
