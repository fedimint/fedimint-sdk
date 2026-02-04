import {
  Transport,
  type TransportLogger,
  type TransportRequest,
} from '@fedimint/types'

import { RpcHandler } from '@fedimint/react-native-bindings'

export class ReactNativeTransport extends Transport {
  logger: TransportLogger = console
  private rpcHandler: RpcHandler

  constructor(dbPath: string) {
    super()
    if (!dbPath) {
      throw new Error('ReactNativeTransport requires a dbPath')
    }
    this.rpcHandler = new RpcHandler(dbPath)
  }

  async postMessage(message: TransportRequest): Promise<void> {
    console.log(
      'ReactNativeTransport postMessage received:',
      JSON.stringify(message),
    )
    const { type, payload, requestId } = message
    try {
      // Handle init - just respond with success since we initialized in constructor
      if (type === 'init') {
        this.messageHandler({
          type: 'data',
          request_id: message.requestId,
          data: true,
        })
        return
      }

      if (
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
        const rustRequest = {
          type: type,
          request_id: requestId,
          payload: payload ?? null,
        }
        const json = JSON.stringify(rustRequest)
        console.log('ReactNativeTransport sending RPC:', json)

        const responseStr = await new Promise<string>((resolve, reject) => {
          try {
            const callback = {
              onResponse: (response: string) => {
                resolve(response)
              },
            }
            this.rpcHandler.rpc(json, callback)
          } catch (e) {
            reject(e)
          }
        })
        console.log('ReactNativeTransport RPC raw response:', responseStr)

        const response = JSON.parse(responseStr)
        console.log(
          'ReactNativeTransport RPC parsed response:',
          JSON.stringify(response),
        )
        if (response.type === 'error') {
          throw new Error(response.error || 'Unknown RPC error')
        }

        this.messageHandler(response)
      } else if (type === 'cleanup') {
        console.log('cleanup message received')
        this.rpcHandler.uniffiDestroy()
      } else {
        this.logger.error('Unknown message type', type)
        this.errorHandler('Unknown message type')
      }
    } catch (error) {
      this.logger.error('RPC Error', error)
      this.messageHandler({
        type: 'error',
        error: error instanceof Error ? error.message : String(error),
        request_id: requestId,
      })
      this.errorHandler(error)
    }
  }
}
