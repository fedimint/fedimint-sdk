// Web Worker for fedimint-client-wasm to run in the browser

// HACK: Fixes vitest browser runner
// TODO: remove once https://github.com/vitest-dev/vitest/pull/6569 lands in a release
// globalThis.__vitest_browser_runner__ = { wrapDynamicImport: (foo) => foo() }

let rpcHandler = null

console.log('Worker - init')

/**
 * Type definitions for the worker messages
 *
 * @typedef {import('../types/worker').WorkerMessageType} WorkerMessageType
 * @typedef {{
 *  type: WorkerMessageType
 *  payload: any
 *  request_id: number
 * }} WorkerMessage
 * @param {{data: WorkerMessage}} event
 */
self.onmessage = async (event) => {
  if (event.data.type === 'init') {
    try {
      // Check if OPFS is supported
      if (!('storage' in navigator) || !('getDirectory' in navigator.storage)) {
        throw new Error('Origin Private File System is not supported')
      }

      // Get the OPFS root directory
      const opfsRoot = await navigator.storage.getDirectory()

      // Create or get the database file
      const fileHandle = await opfsRoot.getFileHandle('fedimint-client.db', {
        create: true,
      })

      // Create a sync access handle
      const dbSyncHandle = await fileHandle.createSyncAccessHandle()

      // Import the WASM module
      const { RpcHandler } = await import(
        '@fedimint/fedimint-client-wasm-bundler'
      )

      // Initialize the RPC handler
      rpcHandler = await new RpcHandler(dbSyncHandle)

      console.log('Worker: WASM module loaded successfully')
      self.postMessage({
        type: 'init_success',
        request_id: event.data.request_id,
      })
    } catch (err) {
      console.error('Worker init failed:', err)
      self.postMessage({
        type: 'init_error',
        error: err.toString(),
        request_id: event.data.request_id,
      })
    }
  } else if (
    event.data.type === 'client_rpc' ||
    event.data.type === 'open_client' ||
    event.data.type === 'close_client' ||
    event.data.type === 'join_federation' ||
    event.data.type === 'cancel_rpc' ||
    event.data.type === 'parse_invite_code' ||
    event.data.type === 'preview_federation' ||
    event.data.type === 'parse_bolt11_invoice' ||
    event.data.type === 'set_mnemonic' ||
    event.data.type === 'generate_mnemonic' ||
    event.data.type === 'get_mnemonic'
  ) {
    // Check if rpcHandler is initialized before calling rpc
    if (!rpcHandler) {
      console.error('Worker: rpcHandler not initialized')
      self.postMessage({
        type: 'error',
        request_id: event.data.request_id,
        error: 'Worker not initialized. Call init first.',
      })
      return
    }

    try {
      console.log(
        'Worker: Processing RPC request:',
        event.data.type,
        event.data.client_name || 'no client_name',
      )
      rpcHandler.rpc(JSON.stringify(event.data), (response) => {
        const parsedResponse = JSON.parse(response)
        console.log(
          'Worker: RPC response for',
          event.data.type,
          ':',
          parsedResponse.kind?.type || parsedResponse.type,
        )
        console.log('Worker: RPC response data:', parsedResponse || 'no data')
        self.postMessage(parsedResponse)
      })
    } catch (err) {
      console.error('Worker RPC error:', err)
      self.postMessage({
        type: 'error',
        error: err.toString(),
        request_id: event.data.request_id,
      })
    }
  } else {
    self.postMessage({
      type: 'error',
      error: `Worker - unimplemented message type: ${event.data.type}`,
      request_id: event.data.request_id,
    })
  }
}
