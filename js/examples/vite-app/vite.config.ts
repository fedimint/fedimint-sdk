import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  optimizeDeps: {
    // The SDK finds its worker and its wasm module with `new URL('…', import.meta.url)`,
    // which Vite's dependency pre-bundling does not follow in dev mode. The production build
    // needs no setting: Vite bundles the worker and copies the module as an asset.
    exclude: ['@fedimint/web-sdk'],
  },
})
