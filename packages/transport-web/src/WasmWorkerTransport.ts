import type { TransportLogger, TransportRequest } from '@fedimint/types'
import { Transport } from '@fedimint/types'

export class WasmWorkerTransport extends Transport {
  private readonly worker: Worker

  logger: TransportLogger = console

  constructor() {
    super()
    this.worker = new Worker(new URL('./worker.js', import.meta.url), {
      type: 'module',
    })
    this.worker.onmessage = (event: MessageEvent) => {
      this.messageHandler(event.data)
    }
    this.worker.onerror = (event: ErrorEvent) => {
      this.errorHandler(event)
      // The worker is in an unknown state (the script failed to load or an
      // uncaught error escaped it), so no in-flight request can be answered
      // anymore. Report a transport-level error (no request_id) so the client
      // fails them all instead of leaving them hanging.
      this.messageHandler({
        type: 'error',
        error: `Wasm worker error: ${event.message || 'unknown'}`,
      })
    }
    this.worker.onmessageerror = () => {
      // A response failed structured deserialization; it cannot be attributed
      // to a request, so the pending ones would otherwise hang forever.
      this.messageHandler({
        type: 'error',
        error: 'Wasm worker response could not be deserialized',
      })
    }
  }

  postMessage(message: TransportRequest) {
    this.worker.postMessage(message)
  }
}
