import { describe, expect, test } from 'vitest'
import type {
  TransportLogger,
  TransportMessage,
  TransportRequest,
} from '@fedimint/types'
import { Transport } from '@fedimint/types'
import { TransportClient } from '../transport/TransportClient'

/**
 * Scripted transport that records outgoing requests and lets tests inject
 * incoming messages, so TransportClient's routing can be tested without a
 * worker or native module.
 */
class FakeTransport extends Transport {
  logger: TransportLogger = console
  sent: TransportRequest[] = []

  postMessage(message: TransportRequest) {
    this.sent.push(message)
  }

  emit(message: TransportMessage) {
    this.messageHandler(message)
  }
}

function pendingRpc(client: TransportClient, transport: FakeTransport) {
  const promise = client.rpcSingle('mint', 'some_method', {}, 'test-client')
  // Requests are numbered by TransportClient; recover the id it assigned.
  const requestId = transport.sent[transport.sent.length - 1].requestId
  return { promise, requestId }
}

describe('TransportClient error routing', () => {
  test('an error attributed to a request rejects only that request', async () => {
    const transport = new FakeTransport()
    const client = new TransportClient(transport)

    const first = pendingRpc(client, transport)
    const second = pendingRpc(client, transport)

    transport.emit({
      type: 'error',
      error: 'boom',
      request_id: first.requestId,
    })
    await expect(first.promise).rejects.toBe('boom')

    transport.emit({
      type: 'data',
      data: { ok: true },
      request_id: second.requestId,
    })
    await expect(second.promise).resolves.toEqual({ ok: true })
  })

  test('a transport-level error (no request_id) fails all pending requests', async () => {
    const transport = new FakeTransport()
    const client = new TransportClient(transport)

    const first = pendingRpc(client, transport)
    const second = pendingRpc(client, transport)

    // What the worker posts when e.g. a wasm panic surfaces as an unhandled
    // rejection: an error message that cannot be attributed to a request.
    transport.emit({ type: 'error', error: 'wasm worker crashed' })

    await expect(first.promise).rejects.toBe('wasm worker crashed')
    await expect(second.promise).rejects.toBe('wasm worker crashed')
    expect(client._getRequestCallbackMap().size).toBe(0)
  })

  test('a throwing consumer callback does not stop the remaining requests from failing', async () => {
    const transport = new FakeTransport()
    const client = new TransportClient(transport)

    // A stream consumer whose error handler throws; registered first so it
    // runs before the well-behaved request's callback.
    client.rpcStream(
      'mint',
      'some_method',
      {},
      'test-client',
      () => {},
      () => {
        throw new Error('badly behaved consumer')
      },
    )
    const pending = pendingRpc(client, transport)

    transport.emit({ type: 'error', error: 'wasm worker crashed' })

    await expect(pending.promise).rejects.toBe('wasm worker crashed')
    expect(client._getRequestCallbackMap().size).toBe(0)
  })

  test('an empty transport-level error message still rejects pending requests', async () => {
    const transport = new FakeTransport()
    const client = new TransportClient(transport)

    const pending = pendingRpc(client, transport)
    // An empty string is falsy, which sendSingleMessage's error branch would
    // ignore — the substitute message keeps the rejection observable.
    transport.emit({ type: 'error', error: '' })

    await expect(pending.promise).rejects.toBe('unknown transport error')
  })

  // Note this covers TransportClient's own state (the callback map is reset),
  // not worker recovery — whether a crashed worker can serve new requests is
  // up to the transport.
  test('the client still routes new requests after a transport-level error', async () => {
    const transport = new FakeTransport()
    const client = new TransportClient(transport)

    const failed = pendingRpc(client, transport)
    transport.emit({ type: 'error', error: 'wasm worker crashed' })
    await expect(failed.promise).rejects.toBe('wasm worker crashed')

    const retried = pendingRpc(client, transport)
    transport.emit({
      type: 'data',
      data: 'recovered',
      request_id: retried.requestId,
    })
    await expect(retried.promise).resolves.toBe('recovered')
  })
})
