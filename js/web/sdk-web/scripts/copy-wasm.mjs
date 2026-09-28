// `tsc` has no `allowJs`, so it emits only TypeScript output and leaves two files behind: the
// wasm-bindgen glue the generated `index.js` imports, and the wasm module. This script copies
// both into `dist/generated`. The glue is committed, so a missing file means a broken checkout
// and the copy fails; the module is a build artifact (.gitignore), present only after a nix
// build, so a missing module only warns.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'

const genDir = new URL('../dist/generated/', import.meta.url)
mkdirSync(genDir, { recursive: true })

const glueSrc = new URL('../src/generated/fedimint_sdk_bg.js', import.meta.url)
if (!existsSync(glueSrc)) {
  throw new Error(
    'sdk-web: missing src/generated/fedimint_sdk_bg.js (committed; checkout is broken)',
  )
}
copyFileSync(glueSrc, new URL('fedimint_sdk_bg.js', genDir))

const wasmSrc = new URL('../src/generated/fedimint_sdk.wasm', import.meta.url)
if (!existsSync(wasmSrc)) {
  console.warn(
    'sdk-web: no src/generated/fedimint_sdk.wasm; run pnpm generate (needs nix)',
  )
} else {
  copyFileSync(wasmSrc, new URL('fedimint_sdk.wasm', genDir))
}
