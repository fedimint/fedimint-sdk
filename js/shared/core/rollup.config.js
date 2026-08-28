/** @type {import('rollup').RollupOptions} */
import typescript from '@rollup/plugin-typescript'
import terser from '@rollup/plugin-terser'
import { dts } from 'rollup-plugin-dts'

export default [
  {
    input: {
      index: 'src/index.ts',
      testing: 'src/testing.ts',
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
    external: [],
  },
  {
    input: {
      index: './dist/dts/index.d.ts',
      testing: './dist/dts/testing.d.ts',
    },
    output: {
      dir: 'dist',
      format: 'es',
      entryFileNames: '[name].d.ts',
    },
    plugins: [dts()],
  },
]
