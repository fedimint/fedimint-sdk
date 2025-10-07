export type JSONValue =
  | string
  | number
  | boolean
  | null
  | { [key: string]: JSONValue }
  | JSONValue[]

export const TRANSPORT_MESSAGE_TYPES = [
  'init',
  'error',
  'cleanup',
  // Defined in fedimint-client-rpc crate: https://github.com/fedimint/fedimint/blob/master/fedimint-client-rpc/src/lib.rs#L60
  // TODO: generate this list automatically
  'set_mnemonic',
  'generate_mnemonic',
  'get_mnemonic',
  'join_federation',
  'open_client',
  'close_client',
  'client_rpc',
  'cancel_rpc',
  'parse_invite_code',
  'parse_bolt11_invoice',
  'preview_federation',
] as const

export type TransportMessageType = (typeof TRANSPORT_MESSAGE_TYPES)[number]

export type TransportRequest = {
  type: TransportMessageType
  requestId?: number
  payload?: JSONValue
}

const TransportResponseTypes = ['data', 'error', 'end', 'log'] as const
type TransportResponseType = (typeof TransportResponseTypes)[number]

export type TransportMessage = {
  type: TransportResponseType
  request_id?: number
} & Record<string, unknown>

export type TransportMessageHandler = (message: TransportMessage) => void

export type TransportErrorHandler = (error: unknown) => void

/**
 * Generic Transport interface for communicating with a specific
 * target of the FedimintClient. Can be Wasm, React Native, Node, etc.
 */
export abstract class Transport {
  protected messageHandler: TransportMessageHandler = () => {}
  protected errorHandler: TransportErrorHandler = () => {}
  abstract logger: TransportLogger
  abstract postMessage(message: TransportRequest): void
  setMessageHandler(handler: TransportMessageHandler): void {
    this.messageHandler = handler
  }
  setErrorHandler(handler: TransportErrorHandler): void {
    this.errorHandler = handler
  }
}

/**
 * Generic logger interface based on console-style logging.
 */
export type TransportLogger = {
  debug(message: string, ...args: any[]): void
  info(message: string, ...args: any[]): void
  warn(message: string, ...args: any[]): void
  error(message: string, ...args: any[]): void
}
