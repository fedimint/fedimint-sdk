import { fileURLToPath } from 'node:url'

import wasm from 'vite-plugin-wasm'
import { defineConfig } from 'vitest/config'
import { playwright } from '@vitest/browser-playwright'

export default defineConfig({
  test: {
    watch: false,
    coverage: {
      provider: 'v8',
      include: ['web/**/*.ts', 'react-native/**/*.ts'],
    },
    projects: [
      {
        plugins: [wasm()],
        test: {
          environment: 'happy-dom',
          name: 'web-sdk',
          include: ['web/web-sdk/src/**/*.browser.test.ts'],
          browser: {
            enabled: true,
            provider: playwright(),
            fileParallelism: false,
            ui: false, // no ui for the browser tests
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
            // devimint exports the faucet port to the environment of the command it
            // execs (`pnpm test` runs under `devimint wasm-test-setup --exec`); the
            // fallback matches devimint's current hard-coded default. `||` so a
            // set-but-empty variable also falls back, like `:-` in the setup script.
            FAUCET: `http://localhost:${process.env.FM_PORT_FAUCET || '15243'}`,
          },
          testTimeout: 180_000,
        },
      },
      {
        test: {
          name: 'unit',
          environment: 'node',
          include: ['web/web-sdk/src/**/*.test.ts'],
          exclude: ['**/*.browser.test.ts'],
        },
      },
      {
        test: {
          name: 'vite-app',
          environment: 'happy-dom',
          include: ['examples/vite-app/src/**/*.test.tsx'],
        },
        resolve: {
          alias: {
            // The tests hand the example fakes of the SDK's objects, so the package comes from
            // its source, which needs neither a build nor the wasm module.
            '@fedimint/web-sdk': fileURLToPath(
              new URL('./web/web-sdk/src/index.ts', import.meta.url),
            ),
          },
        },
      },
      {
        test: {
          name: 'react-native',
          environment: 'node',
          include: ['react-native/react-native/**/*.test.ts'],
        },
        resolve: {
          alias: {
            // The real bindings module is ubrn-generated and needs a compiled native library;
            // unit tests run against a stub instead.
            '@fedimint/react-native-bindings': fileURLToPath(
              new URL(
                './react-native/react-native/src/__tests__/bindings-stub.ts',
                import.meta.url,
              ),
            ),
          },
        },
      },
    ],
  },
  optimizeDeps: {
    exclude: ['@fedimint/web-sdk'],
  },
})
