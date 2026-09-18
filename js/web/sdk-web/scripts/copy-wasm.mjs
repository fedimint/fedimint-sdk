// Copies the staged module next to the compiled entrypoint. `tsc` does not carry non-TS files,
// and the module is a build artifact (.gitignore), so a checkout without a nix build has none
// to copy; that is a warning, not a failure, because every JS-only job builds this package.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'

const src = new URL('../src/generated/fedimint_sdk.wasm', import.meta.url)
const dst = new URL('../dist/generated/fedimint_sdk.wasm', import.meta.url)
if (!existsSync(src)) {
  console.warn(
    'sdk-web: no src/generated/fedimint_sdk.wasm; run pnpm generate (needs nix)',
  )
} else {
  mkdirSync(new URL('../dist/generated/', import.meta.url), { recursive: true })
  copyFileSync(src, dst)
}
