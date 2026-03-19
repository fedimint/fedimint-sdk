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
    this.logger.info(
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
          request_id: requestId,
          type: type,
          ...(typeof payload === 'object' && payload !== null ? payload : {}),
        }
        const json = JSON.stringify(rustRequest)
        console.info('ReactNativeTransport sending RPC:', json)

        const callback = {
          onResponse: (responseStr: string) => {
            try {
              const response = JSON.parse(responseStr)
              console.info(
                'ReactNativeTransport RPC parsed response:',
                JSON.stringify(response),
              )

              if (response.type === 'error') {
                this.messageHandler({
                  type: 'error',
                  error: response.error || 'Unknown RPC error',
                  request_id: requestId,
                })
                this.errorHandler(
                  new Error(response.error || 'Unknown RPC error'),
                )
                return
              }

              // Forward the full Rust response (data, end, aborted)
              // directly to the messageHandler
              this.messageHandler(response)
            } catch (parseError) {
              this.logger.error('RPC response parse error', parseError)
              this.messageHandler({
                type: 'error',
                error:
                  parseError instanceof Error
                    ? parseError.message
                    : String(parseError),
                request_id: requestId,
              })
              this.errorHandler(parseError)
            }
          },
        }

        try {
          this.rpcHandler.rpc(json, callback)
        } catch (e) {
          this.logger.error('RPC call error', e)
          this.messageHandler({
            type: 'error',
            error: e instanceof Error ? e.message : String(e),
            request_id: requestId,
          })
          this.errorHandler(e)
        }
      } else if (type === 'cleanup') {
        this.logger.info('cleanup message received')
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
