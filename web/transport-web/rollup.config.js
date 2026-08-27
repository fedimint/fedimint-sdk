/** @type {import('rollup').RollupOptions} */
import typescript from '@rollup/plugin-typescript'
import terser from '@rollup/plugin-terser'
import { dts } from 'rollup-plugin-dts'

export default [
  {
    input: {
      index: 'src/index.ts',
      worker: 'src/worker.js',
    },
    output: {
      dir: 'dist',
      format: 'esm',
      entryFileNames: '[name].js',
      chunkFileNames: '[name].js',
      assetFileNames: '[name].[ext]',
      sourcemap: true,
    },
    plugins: [typescript(), terser()],
    external: [
      '@fedimint/core',
      '@fedimint/fedimint-client-wasm-bundler',
      '@fedimint/types',
    ],
  },
  {
    input: './dist/dts/index.d.ts',
    output: [{ file: 'dist/index.d.ts', format: 'es' }],
    plugins: [dts()],
  },
]
