import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

const apiProxyTarget = process.env.VITE_API_PROXY_TARGET ?? 'http://127.0.0.1:3001';

const apiProxyConfig = {
  '/api': apiProxyTarget
};

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: apiProxyConfig
  },
  preview: {
    proxy: apiProxyConfig
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['src/test/setup.ts']
  }
});