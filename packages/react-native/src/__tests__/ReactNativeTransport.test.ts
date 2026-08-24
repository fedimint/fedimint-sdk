import type { TransportMessage } from '@fedimint/types'
import { describe, expect, it } from 'vitest'

import { ReactNativeTransport } from '../ReactNativeTransport'
import { RpcHandler } from './rpc-handler-stub'

const silentLogger = {
  debug: () => {},
  info: () => {},
  warn: () => {},
  error: () => {},
}

function newTransport() {
  const transport = new ReactNativeTransport('/tmp/fedimint-test-db')
  transport.logger = silentLogger
  const messages: TransportMessage[] = []
  const errors: unknown[] = []
  transport.setMessageHandler((message) => messages.push(message))
  transport.setErrorHandler((error) => errors.push(error))
  const handler = RpcHandler.instances.at(-1)!
  return { transport, handler, messages, errors }
}

describe('ReactNativeTransport', () => {
  it('requires a dbPath', () => {
    expect(() => new ReactNativeTransport('')).toThrow(/dbPath/)
  })

  it('speaks the fedimint-client-rpc wire format', async () => {
    const { transport, handler } = newTransport()

    await transport.postMessage({
      type: 'parse_invite_code',
      requestId: 7,
      payload: { invite_code: 'fed11qgq...' },
    })

    // Must match the serde representation of the Rust RpcRequest type:
    // snake_case request_id, internally tagged type, flattened payload.
    expect(handler.requests).toHaveLength(1)
    expect(handler.requests[0]!.request).toEqual({
      request_id: 7,
      type: 'parse_invite_code',
      invite_code: 'fed11qgq...',
    })
  })

  it('answers init locally without touching the native handler', async () => {
    const { transport, handler, messages, errors } = newTransport()

    await transport.postMessage({ type: 'init', requestId: 1 })

    expect(messages).toEqual([{ type: 'data', request_id: 1, data: true }])
    expect(handler.requests).toHaveLength(0)
    expect(errors).toHaveLength(0)
  })

  it('forwards data and end responses untouched', async () => {
    const { transport, handler, messages, errors } = newTransport()

    await transport.postMessage({ type: 'has_mnemonic_set', requestId: 2 })
    handler.respond(0, { request_id: 2, type: 'data', data: false })
    handler.respond(0, { request_id: 2, type: 'end' })

    expect(messages).toEqual([
      { request_id: 2, type: 'data', data: false },
      { request_id: 2, type: 'end' },
    ])
    expect(errors).toHaveLength(0)
  })

  it('routes error responses to both handlers', async () => {
    const { transport, handler, messages, errors } = newTransport()

    await transport.postMessage({ type: 'get_mnemonic', requestId: 3 })
    handler.respond(0, { request_id: 3, type: 'error', error: 'boom' })

    expect(messages).toEqual([{ type: 'error', error: 'boom', request_id: 3 }])
    expect(errors).toHaveLength(1)
    expect((errors[0] as Error).message).toBe('boom')
  })

  it('reports synchronous rpc failures as errors', async () => {
    const { transport, handler, messages, errors } = newTransport()
    handler.throwOnRpc = new Error('native call failed')

    await transport.postMessage({ type: 'generate_mnemonic', requestId: 4 })

    expect(messages).toEqual([
      { type: 'error', error: 'native call failed', request_id: 4 },
    ])
    expect(errors).toHaveLength(1)
  })

  it('destroys the native handler on cleanup', async () => {
    const { transport, handler } = newTransport()

    await transport.postMessage({ type: 'cleanup' })

    expect(handler.destroyed).toBe(true)
  })
})
