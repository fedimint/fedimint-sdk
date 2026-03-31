import { sveltekit } from '@sveltejs/kit/vite'
import { defineConfig } from 'vite'
import wasm from 'vite-plugin-wasm'

// https://vite.dev/config/
export default defineConfig({
  plugins: [sveltekit(), wasm()],

  // These worker settings are required
  worker: {
    format: 'es',
    plugins: () => [
      wasm(), // Required for wasm
    ],
  },
  build: {
    target: 'esnext',
    sourcemap: true,
    minify: false,
  },
  optimizeDeps: {
    exclude: ['@fedimint/core', '@fedimint/transport-web'],
  },
})
