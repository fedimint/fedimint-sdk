# @fedimint/transport-web

Web Worker transport bindings for the Fedimint client. This package hosts the browser-specific transport implementation and worker bundle that powers `TransportClient` from `@fedimint/core`.

```ts
import { WasmWorkerTransport } from '@fedimint/transport-web'
import { WalletDirector } from '@fedimint/core'

const director = new WalletDirector(new WasmWorkerTransport())
```

The worker script is exposed as `worker.js` from the package root. Use this package from browser environments only; provide a different transport implementation on other platforms.
