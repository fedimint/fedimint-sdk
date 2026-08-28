import fs from 'fs'
import path from 'path'
import { createRequire } from 'module'

const require = createRequire(import.meta.url)
const CORE_LIB_DIST = path.dirname(require.resolve('@fedimint/core'))
const TRANSPORT_DIST = path.dirname(require.resolve('@fedimint/transport-web'))
const WASM_DIST = path.resolve(
  CORE_LIB_DIST,
  '../node_modules/@fedimint/fedimint-client-wasm',
)

function printErr(err) {
  if (err) return console.log(err)
}

// fs.copyFile(path.resolve(CORE_LIB_DIST, 'index.js'), './index.js', printErr)
fs.copyFile(path.resolve(TRANSPORT_DIST, 'worker.js'), './worker.js', printErr)
// fs.copyFile(path.resolve(WASM_DIST, 'fedimint_client_wasm_bg.wasm'), './fedimint_client_wasm_bg.wasm', printErr)

// fs.copyFile(path.resolve(WASM_DIST, 'fedimint_client_wasm.js'), './fedimint_client_wasm.js', printErr)
// fs.copyFile(path.resolve(WASM_DIST, 'fedimint_client_wasm_bg.js'), './fedimint_client_wasm_bg.js', printErr)
