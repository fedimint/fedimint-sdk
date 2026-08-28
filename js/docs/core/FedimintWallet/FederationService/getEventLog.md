# Get Event Log

### `federation.getEventLog(params?: EventLogParams)`

Returns persisted client event log entries for the connected federation. Pass
`pos` to start reading from a specific event id and `limit` to cap the number
of returned entries. If no `limit` is given, `10_000` is used as the limit.

```ts twoslash
type EventLogParams = {
  pos?: number | null
  limit?: number | null
}

type PersistedLogEntry = {
  id: number
  kind: string
  module: [string, number] | null
  ts_usecs: number
  payload: unknown
}
```

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const eventLog = await wallet.federation.getEventLog({
  pos: 0,
  limit: 20,
})
console.log('event log entries are: ', eventLog)
```
