// import { defineWorkspace } from 'vitest/config'
// import wasm from 'vite-plugin-wasm'

// export default defineWorkspace([
//   {
//     plugins: [wasm()],
//     test: {
//       environment: 'happy-dom',
//       name: 'core',
//       include: ['shared/core/**/*.test.ts'],
//       exclude: ['tools/create-fedimint-app/**/*.test.ts'],
//       browser: {
//         enabled: true,
//         provider: 'playwright',
//         ui: false, // no ui for the core library
//         api: {
//           port: 63315,
//         },
//         screenshotFailures: false,
//         instances: [
//           {
//             browser: 'chromium',
//             headless: true,
//           },
//         ],
//       },
//       env: {
//         FAUCET: `http://localhost:15243`,
//       },
//     },
//   },
//   {
//     test: {
//       name: 'cli',
//       environment: 'happy-dom',
//       include: ['tools/create-fedimint-app/__tests__/*.test.ts'],
//       exclude: ['tools/create-fedimint-app/__tests__/subfolder'],
//       isolate: true,
//       testTimeout: 20000,
//     },
//   },
// ])
