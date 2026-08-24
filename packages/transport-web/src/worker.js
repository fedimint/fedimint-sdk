// Web Worker for fedimint-client-wasm to run in the browser

// dynamically imported rpcHandler
/** @type {import('@fedimint/fedimint-client-wasm-bundler').RpcHandler} */
let rpcHandler
let dbSyncHandle = null
let dbFilename = null

console.log('Worker - init')

/**
 * Type definitions for the worker messages
 *
 * @typedef {import('@fedimint/types').TransportMessageType} WorkerMessageType
 * @typedef {{
 *  type: WorkerMessageType
 *  payload: any
 *  requestId: number
 * }} WorkerMessage
 * @param {{data: WorkerMessage}} event
 */
self.onmessage = async (event) => {
  const { type, payload, requestId } = event.data

  try {
    if (type === 'init') {
      const RpcHandler = (
        await import('@fedimint/fedimint-client-wasm-bundler')
      ).RpcHandler

      const root = await navigator.storage.getDirectory()
      // Allows to pass in a filename for testing
      const filename = payload?.dbPath || 'fedimint.db'
      dbFilename = filename
      const dbFileHandle = await root.getFileHandle(filename, {
        create: true,
      })
      dbSyncHandle = await dbFileHandle.createSyncAccessHandle()
      rpcHandler = await new RpcHandler(dbSyncHandle)
      self.postMessage({
        type: 'initialized',
        data: { filename },
        request_id: requestId,
      })
    } else if (
      type === 'set_mnemonic' ||
      type === 'generate_mnemonic' ||
      type === 'get_mnemonic' ||
      type === 'join_federation' ||
      type === 'open_client' ||
      type === 'close_client' ||
      type === 'client_rpc' ||
      type === 'cancel_rpc' ||
      type === 'parse_invite_code' ||
      type === 'parse_bolt11_invoice' ||
      type === 'preview_federation' ||
      type === 'parse_oob_notes' ||
      type === 'has_mnemonic_set'
    ) {
      self.postMessage({
        type: 'log',
        level: 'info',
        message: `RPC received`,
        request_type: type,
        requestId,
        payload,
      })
      if (!rpcHandler) {
        self.postMessage({
          type: 'error',
          error: 'rpcHandler not initialized',
          request_id: requestId,
        })
        return
      }
      const rpcRequest = JSON.stringify({
        request_id: requestId,
        type,
        ...payload,
      })
      rpcHandler.rpc(rpcRequest, (response) =>
        self.postMessage(JSON.parse(response)),
      )
    } else if (type === 'cleanup') {
      console.log('cleanup message received')
      dbSyncHandle?.close()
      rpcHandler?.free()
      self.postMessage({
        type: 'cleanup',
        data: { filename: dbFilename },
        request_id: requestId,
      })
      close()
    } else {
      self.postMessage({
        type: 'error',
        error: 'Unknown message type',
        request_id: requestId,
      })
    }
  } catch (e) {
    console.error('ERROR', e)
    self.postMessage({
      type: 'error',
      error: e instanceof Error ? e.message : String(e),
      request_id: requestId,
    })
  }
}

/** @param {unknown} value */
function describeError(value) {
  if (value instanceof Error) {
    // Prefer the stack: for a wasm panic it names the wasm frames, which is
    // the only clue to what actually crashed.
    return value.stack || value.message || String(value)
  }
  return String(value) || 'unknown error'
}

// A crash outside the message handler is otherwise invisible to the main thread and
// the request that triggered it would just never resolve. The main offender is a
// panic in the wasm client: it aborts the async task spawned for the RPC, which
// surfaces here as an unhandled rejection (`RuntimeError: unreachable`). Report such
// crashes as transport-level errors (no request_id) so the client can fail all
// in-flight requests with the real error. This deliberately treats every uncaught
// error/rejection as fatal: the worker's only async work is the wasm client and its
// storage, and after a wasm trap the instance state cannot be trusted anyway.
// preventDefault() marks the event handled so it is not additionally re-reported
// through the parent's `worker.onerror` (which would fail everything a second
// time); console.error keeps the full text visible in browser/CI logs.
self.addEventListener('error', (event) => {
  event.preventDefault()
  const error = `Uncaught error in wasm worker: ${event.message || describeError(event.error)}`
  console.error(error)
  self.postMessage({ type: 'error', error })
})

self.addEventListener('unhandledrejection', (event) => {
  event.preventDefault()
  const error = `Unhandled rejection in wasm worker: ${describeError(event.reason)}`
  console.error(error)
  self.postMessage({ type: 'error', error })
})
