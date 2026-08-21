import { fileURLToPath } from 'node:url'

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
          name: 'integration-tests',
          include: ['packages/integration-tests/**/*.test.ts'],
          exclude: ['packages/create-fedimint-app/**/*.test.ts'],
          browser: {
            enabled: true,
            provider: 'playwright',
            fileParallelism: false,
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
            // devimint exports the faucet port to the environment of the command it
            // execs (`pnpm test` runs under `devimint wasm-test-setup --exec`); the
            // fallback matches devimint's current hard-coded default. `||` so a
            // set-but-empty variable also falls back, like `:-` in the setup script.
            FAUCET: `http://localhost:${process.env.FM_PORT_FAUCET || '15243'}`,
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
      {
        test: {
          name: 'unit',
          environment: 'node',
          include: ['packages/core/**/*.test.ts'],
        },
        resolve: {
          alias: {
            // Type-only workspace dependency; alias to source so the package
            // does not have to be built before running unit tests.
            '@fedimint/types': fileURLToPath(
              new URL('./packages/types/src/index.ts', import.meta.url),
            ),
          },
        },
      },
      {
        test: {
          name: 'react-native',
          environment: 'node',
          include: ['packages/react-native/**/*.test.ts'],
        },
        resolve: {
          alias: {
            // The real bindings module is ubrn-generated and needs a compiled
            // native library; unit tests run against a stub instead. Types are
            // aliased to source so no workspace build is needed beforehand.
            '@fedimint/react-native-bindings': fileURLToPath(
              new URL(
                './packages/react-native/src/__tests__/rpc-handler-stub.ts',
                import.meta.url,
              ),
            ),
            '@fedimint/types': fileURLToPath(
              new URL('./packages/types/src/index.ts', import.meta.url),
            ),
          },
        },
      },
    ],
  },
  optimizeDeps: {
    exclude: ['@fedimint/core'],
  },
})
