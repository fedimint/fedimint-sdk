import wasm from 'vite-plugin-wasm'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    watch: false,
    coverage: {
      provider: 'v8',
      include: ['packages/**/*.ts'],
    },
    projects: [
      {
        plugins: [wasm()],
        test: {
          environment: 'happy-dom',
          name: 'core',
          include: ['packages/core-web/**/*.test.ts'],
          exclude: ['packages/create-fedimint-app/**/*.test.ts'],
          browser: {
            enabled: true,
            provider: 'playwright',
            ui: false, // no ui for the core library
            api: {
              port: 63315,
            },
            screenshotFailures: false,
            instances: [
              {
                browser: 'chromium',
                headless: true,
              },
            ],
          },
          env: {
            FAUCET: `http://localhost:15243`,
          },
        },
      },
      {
        test: {
          name: 'cli',
          environment: 'happy-dom',
          include: ['packages/create-fedimint-app/__tests__/*.test.ts'],
          exclude: ['packages/create-fedimint-app/__tests__/subfolder'],
          isolate: true,
          testTimeout: 20000,
        },
      },
    ],
  },
  optimizeDeps: {
    exclude: ['@fedimint/core-web'],
  },
})
