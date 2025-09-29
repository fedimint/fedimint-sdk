import type {
  CancelFunction,
  JSONValue,
  ModuleKind,
  StreamError,
  StreamResult,
} from '../types'
import { Logger } from '../utils/logger'
import type {
  Transport,
  TransportMessage,
  TransportMessageType,
} from '@fedimint/transport-types'

/**
 * Handles communication with a generic transport.
 * Must be instantiated with a platform-specific transport. (wasm for web, react native, etc.)
 */
export class TransportClient {
  // Generic Transport. Can be wasm, react native, node, etc.
  private readonly transport: Transport
  private requestCounter = 0
  private requestCallbacks = new Map<number, (value: any) => void>()
  private initPromise: Promise<boolean> | undefined = undefined
  logger: Logger

  /**
   * @summary Constructor for the TransportClient
   * @param transport - The platform-specific transport to use. (wasm for web, react native, etc.)
   */
  constructor(transport: Transport) {
    this.transport = transport
    this.logger = new Logger(transport.logger)
    this.transport.setMessageHandler(this.handleTransportMessage)
    this.transport.setErrorHandler(this.handleTransportError)
    this.logger.info('TransportClient instantiated')
    this.logger.debug('TransportClient transport', transport)
  }

  // Idempotent setup - Loads the wasm module
  initialize() {
    if (this.initPromise) return this.initPromise
    this.initPromise = this.sendSingleMessage('init')
    return this.initPromise
  }

  private handleLogMessage(message: TransportMessage) {
    const { type, level, message: logMessage, ...data } = message
    this.logger.info(String(level), String(logMessage), data)
  }

  private handleTransportError = (error: unknown) => {
    this.logger.error('TransportClient error', error)
  }

  private handleTransportMessage = (message: TransportMessage) => {
    const { type, requestId, ...data } = message
    if (type === 'log') {
      this.handleLogMessage(message)
    }
    const streamCallback =
      requestId !== undefined ? this.requestCallbacks.get(requestId) : undefined
    // TODO: Handle errors... maybe have another callbacks list for errors?
    this.logger.debug('TransportClient - handleTransportMessage', message)
    if (streamCallback) {
      streamCallback(data) // {data: something} OR {error: something}
    } else if (requestId !== undefined) {
      this.logger.warn(
        'TransportClient - handleTransportMessage - received message with no callback',
        requestId,
        message,
      )
    }
  }

  // TODO: Handle errors... maybe have another callbacks list for errors?
  // TODO: Handle timeouts
  // TODO: Handle multiple errors

  sendSingleMessage<
    Response extends JSONValue = JSONValue,
    Payload extends JSONValue = JSONValue,
  >(type: TransportMessageType, payload?: Payload) {
    return new Promise<Response>((resolve, reject) => {
      const requestId = ++this.requestCounter
      this.logger.debug(
        'TransportClient - sendSingleMessage',
        requestId,
        type,
        payload,
      )
      this.requestCallbacks.set(
        requestId,
        (response: StreamResult<Response>) => {
          this.requestCallbacks.delete(requestId)
          this.logger.debug(
            'TransportClient - sendSingleMessage - response',
            requestId,
            response,
          )
          if (response.data) resolve(response.data)
          else if (response.error) reject(response.error)
          else
            this.logger.warn(
              'TransportClient - sendSingleMessage - malformed response',
              requestId,
              response,
            )
        },
      )
      this.transport.postMessage({ type, payload, requestId })
    })
  }

  /**
   * @summary Initiates an RPC stream with the specified module and method.
   *
   * @description
   * This function sets up an RPC stream by sending a request to a worker and
   * handling responses asynchronously. It ensures that unsubscription is handled
   * correctly, even if the unsubscribe function is called before the subscription
   * is fully established, by deferring the unsubscription attempt using `setTimeout`.
   *
   * The function operates in a non-blocking manner, leveraging Promises to manage
   * asynchronous operations and callbacks to handle responses.
   *
   *
   * @template Response - The expected type of the successful response.
   * @template Body - The type of the request body.
   * @param module - The module kind to interact with.
   * @param method - The method name to invoke on the module.
   * @param body - The request payload.
   * @param onSuccess - Callback invoked with the response data on success.
   * @param onError - Callback invoked with error information if an error occurs.
   * @param onEnd - Optional callback invoked when the stream ends.
   * @returns A function that can be called to cancel the subscription.
   *
   */
  rpcStream<
    Response extends JSONValue = JSONValue,
    Body extends JSONValue = JSONValue,
  >(
    module: ModuleKind,
    method: string,
    body: Body,
    onSuccess: (res: Response) => void,
    onError: (res: StreamError['error']) => void,
    onEnd: () => void = () => {},
  ): CancelFunction {
    const requestId = ++this.requestCounter
    this.logger.debug(
      'TransportClient - rpcStream',
      requestId,
      module,
      method,
      body,
    )
    let unsubscribe: (value: void) => void = () => {}
    let isSubscribed = false

    const unsubscribePromise = new Promise<void>((resolve) => {
      unsubscribe = () => {
        if (isSubscribed) {
          // If already subscribed, resolve immediately to trigger unsubscription
          resolve()
        } else {
          // If not yet subscribed, defer the unsubscribe attempt to the next event loop tick
          // This ensures that subscription setup has time to complete
          setTimeout(() => unsubscribe(), 0)
        }
      }
    })

    // Initiate the inner RPC stream setup asynchronously
    this._rpcStreamInner(
      requestId,
      module,
      method,
      body,
      onSuccess,
      onError,
      onEnd,
      unsubscribePromise,
    ).then(() => {
      isSubscribed = true
    })

    return unsubscribe
  }

  private async _rpcStreamInner<
    Response extends JSONValue = JSONValue,
    Body extends JSONValue = JSONValue,
  >(
    requestId: number,
    module: ModuleKind,
    method: string,
    body: Body,
    onSuccess: (res: Response) => void,
    onError: (res: StreamError['error']) => void,
    onEnd: () => void = () => {},
    unsubscribePromise: Promise<void>,
    // Unsubscribe function
  ) {
    this.requestCallbacks.set(requestId, (response: StreamResult<Response>) => {
      if (response.error !== undefined) {
        onError(response.error)
      } else if (response.data !== undefined) {
        onSuccess(response.data)
      } else if (response.end !== undefined) {
        this.requestCallbacks.delete(requestId)
        onEnd()
      }
    })
    this.transport.postMessage({
      type: 'rpc',
      payload: { module, method, body },
      requestId,
    })

    unsubscribePromise.then(() => {
      this.transport.postMessage({
        type: 'unsubscribe',
        requestId,
      })
      this.requestCallbacks.delete(requestId)
    })
  }

  rpcSingle<
    Response extends JSONValue = JSONValue,
    Error extends string = string,
  >(module: ModuleKind, method: string, body: JSONValue) {
    this.logger.debug('TransportClient - rpcSingle', module, method, body)
    return new Promise<Response>((resolve, reject) => {
      this.rpcStream<Response>(module, method, body, resolve, reject)
    })
  }

  async cleanup() {
    await this.sendSingleMessage('cleanup')
    this.requestCounter = 0
    this.initPromise = undefined
    this.requestCallbacks.clear()
  }

  // For Testing
  _getRequestCounter() {
    return this.requestCounter
  }
  _getRequestCallbackMap() {
    return this.requestCallbacks
  }
}
