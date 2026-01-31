import {
  Transport,
  type TransportLogger,
  type TransportRequest,
} from '@fedimint/types';

export class ReactNativeTransport extends Transport {
  logger: TransportLogger = console;
  rpcHandler: any = null;

  constructor() {
    super();
  }

  async postMessage(message: TransportRequest): Promise<void> {
    console.log(
      'ReactNativeTransport postMessage received:',
      JSON.stringify(message)
    );
    const { type, payload, requestId } = message;
    try {
      if (type === 'init') {
        const payload = message.payload as { dbPath?: string } | undefined;
        if (payload?.dbPath) {
          const RpcHandler = (
            await import('../generated/fedimint_client_uniffi')
          ).RpcHandler;

          console.log(
            'ReactNativeTransport: init received with filename, calling setup:',
            payload.dbPath
          );

          if (this.rpcHandler) {
            console.log('RPC Service already initialized');
            return;
          }
          this.rpcHandler = new RpcHandler(payload.dbPath);
          // Respond with success
          this.messageHandler({
            type: 'data',
            request_id: message.requestId,
            data: true,
          });
          return;
        } else {
          this.logger.error(
            'ReactNativeTransport: init received without file path'
          );
        }
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
        if (!this.rpcHandler) {
          this.logger.error('ReactNativeTransport: rpcHandler not initialized');
          this.errorHandler('rpcHandler not initialized');
          return;
        }
        const rustRequest = {
          type: type,
          request_id: requestId,
          payload: payload ?? null, // Ensure payload exists as null if undefined
        };
        const json = JSON.stringify(rustRequest);
        console.log('ReactNativeTransport sending RPC:', json);

        const responseStr = await new Promise<string>((resolve, reject) => {
          try {
            const callback = {
              onResponse: (response: string) => {
                resolve(response);
              },
            };
            this.rpcHandler.rpc(json, callback);
          } catch (e) {
            reject(e);
          }
        });
        console.log('ReactNativeTransport RPC raw response:', responseStr);

        const response = JSON.parse(responseStr);
        console.log(
          'ReactNativeTransport RPC parsed response:',
          JSON.stringify(response)
        );
        if (response.type === 'error') {
          throw new Error(response.error || 'Unknown RPC error');
        }

        this.messageHandler(response);
      } else if (type === 'cleanup') {
        console.log('cleanup message received');
        this.rpcHandler?.free();
      } else {
        this.logger.error('Unknown message type', type);
        this.errorHandler('Unknown message type');
      }
    } catch (error) {
      this.logger.error('RPC Error', error);
      // Ensure error is propagated with the structure expected by the listener
      this.messageHandler({
        type: 'error',
        error: error instanceof Error ? error.message : String(error),
        request_id: requestId,
      });
      this.errorHandler(error);
    }
  }
}