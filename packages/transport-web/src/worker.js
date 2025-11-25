// Web Worker for fedimint-client-wasm to run in the browser

// HACK: Fixes vitest browser runner
// TODO: remove once https://github.com/vitest-dev/vitest/pull/6569 lands in a release
globalThis.__vitest_browser_runner__ = { wrapDynamicImport: (foo) => foo() }

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
      const filename = payload?.filename || 'fedimint.db'
      dbFilename = filename
      const dbFileHandle = await root.getFileHandle(filename, {
        create: true,
      })
      dbSyncHandle = await dbFileHandle.createSyncAccessHandle()
      rpcHandler = new RpcHandler(dbSyncHandle)
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
      type === 'parse_oob_notes'
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
    self.postMessage({ type: 'error', error: e.message, request_id: requestId })
  }
}

self.onerror = (e) => {
  self.postMessage({ type: 'error', error: e.message })
}
