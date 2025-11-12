import { TransportClient } from './transport'
import { type LogLevel } from './utils/logger'
import { FederationConfig, JSONValue } from './types'
import { Transport } from '@fedimint/types'
import { FedimintWallet } from './FedimintWallet'

export class WalletDirector {
  // Protected to allow for TestWalletDirector to access the client
  protected _client: TransportClient

  /**
   * Creates a new instance of WalletDirector.
   *
   * @param {Transport} [transport] - Optional worker client instance. Provide your
   *                         own to use a custom transport (e.g. React Native).
   *
   * @param {boolean} lazy - If true, delays Web Worker and WebAssembly initialization
   *                         until needed. Default is false.
   */
  constructor(transport: Transport, lazy: boolean = false) {
    if (!transport) {
      throw new Error('WalletDirector requires a transport implementation')
    }
    this._client = new TransportClient(transport)
    this._client.logger.info('WalletDirector instantiated')
    if (!lazy) {
      this.initialize()
    }
  }

  async initialize() {
    this._client.logger.info('Initializing TransportClient')
    await this._client.initialize()
    this._client.logger.info('TransportClient initialized')
  }

  // TODO: Make this stateful... handle listing/joining/opening/closing wallets at this level
  async createWallet() {
    await this._client.initialize()
    return new FedimintWallet(this._client)
  }

  async previewFederation(inviteCode: string) {
    await this._client.initialize()
    const response = this._client.sendSingleMessage<{
      config: FederationConfig
      federation_id: string
    }>('preview_federation', { invite_code: inviteCode })
    return response
  }

  /**
   * Sets the log level for the library.
   * @param level The desired log level ('DEBUG', 'INFO', 'WARN', 'ERROR', 'NONE').
   */
  setLogLevel(level: LogLevel) {
    this._client.logger.setLevel(level)
    this._client.logger.info(`Log level set to ${level}.`)
  }

  /**
   * Parses a federation invite code and retrieves its details.
   *
   * This method sends the provided invite code to the TransportClient for parsing.
   * The response includes the federation_id and url.
   *
   * @param {string} inviteCode - The invite code to be parsed.
   * @returns {Promise<{ federation_id: string, url: string}>}
   *          A promise that resolves to an object containing:
   *          - `federation_id`: The id of the feder.
   *          - `url`: One of the apipoints to connect to the federation
   *
   * @throws {Error} If the TransportClient encounters an issue during the parsing process.
   *
   * @example
   * const inviteCode = "example-invite-code";
   * const parsedCode = await wallet.parseInviteCode(inviteCode);
   * console.log(parsedCode.federation_id, parsedCode.url);
   */
  async parseInviteCode(inviteCode: string) {
    await this._client.initialize()
    const response = await this._client.sendSingleMessage<{
      type: string
      data: JSONValue
      requestId: number
    }>('parse_invite_code', { invite_code: inviteCode })
    return response
  }

  /**
   * Parses a BOLT11 Lightning invoice and retrieves its details.
   *
   * This method sends the provided invoice string to the TransportClient for parsing.
   * The response includes details such as the amount, expiry, and memo.
   *
   * @param {string} invoiceStr - The BOLT11 invoice string to be parsed.
   * @returns {Promise<{ amount: string, expiry: number, memo: string }>}
   *          A promise that resolves to an object containing:
   *          - `amount`: The amount specified in the invoice.
   *          - `expiry`: The expiry time of the invoice in seconds.
   *          - `memo`: A description or memo attached to the invoice.
   *
   * @throws {Error} If the TransportClient encounters an issue during the parsing process.
   *
   * @example
   * const invoiceStr = "lnbc1...";
   * const parsedInvoice = await wallet.parseBolt11Invoice(invoiceStr);
   * console.log(parsedInvoice.amount, parsedInvoice.expiry, parsedInvoice.memo);
   */
  async parseBolt11Invoice(invoiceStr: string) {
    await this._client.initialize()
    const response = await this._client.sendSingleMessage<{
      type: string
      data: JSONValue
      requestId: number
    }>('parse_bolt11_invoice', { invoice: invoiceStr })
    return response
  }

  /**
   * Generates and sets a new mnemonic phrase.
   * @returns {Promise<string[]>} A promise that resolves to the generated mnemonic phrase.
   */
  async generateMnemonic(): Promise<string[]> {
    const result = await this._client.sendSingleMessage<{ mnemonic: string[] }>(
      'generate_mnemonic',
    )
    return result.mnemonic
  }

  /**
   * Retrieves the current mnemonic phrase.
   * @returns {Promise<string[]>} A promise that resolves to the current mnemonic phrase.
   */
  async getMnemonic(): Promise<string[]> {
    const result = await this._client.sendSingleMessage<{ mnemonic: string[] }>(
      'get_mnemonic',
    )
    return result.mnemonic
  }

  /**
   * Sets the mnemonic phrase.
   * @param {string[]} words - The mnemonic words to set.
   * @returns {Promise<boolean>} A promise that resolves to true if the mnemonic was set successfully.
   */
  async setMnemonic(words: string[]): Promise<boolean> {
    const result = await this._client.sendSingleMessage<{ success: boolean }>(
      'set_mnemonic',
      { words },
    )
    return result.success
  }
}
