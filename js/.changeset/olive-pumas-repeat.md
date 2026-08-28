---
'@fedimint/fedimint-client-wasm-bundler': patch
'@fedimint/fedimint-client-wasm-web': patch
---

Rebuild the wasm client from a fedimint revision that installs a panic hook and fixes the wallet write conflict behind it, so a peg-in issued right after joining a federation no longer crashes the worker
