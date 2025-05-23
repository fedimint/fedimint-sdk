import { WorkerClient } from './worker'
import {
  BalanceService,
  MintService,
  LightningService,
  FederationService,
  RecoveryService,
  WalletService,
} from './services'
import { logger, type LogLevel } from './utils/logger'
import { FederationConfig, JSONValue } from './types'

const DEFAULT_CLIENT_NAME = 'fm-default' as const

export class FedimintWallet {
  private _client: WorkerClient

  public balance: BalanceService
  public mint: MintService
  public lightning: LightningService
  public federation: FederationService
  public recovery: RecoveryService
  public wallet: WalletService

  private _openPromise: Promise<void> | undefined = undefined
  private _resolveOpen: () => void = () => {}
  private _isOpen: boolean = false

  /**
   * Creates a new instance of FedimintWallet.
   *
   * This constructor initializes a FedimintWallet instance, which manages communication
   * with a Web Worker. The Web Worker is responsible for running WebAssembly code that
   * handles the core Fedimint Client operations.
   *
   * (default) When not in lazy mode, the constructor immediately initializes the
   * Web Worker and begins loading the WebAssembly module in the background. This
   * allows for faster subsequent operations but may increase initial load time.
   *
   * In lazy mode, the Web Worker and WebAssembly initialization are deferred until
   * the first operation that requires them, reducing initial overhead at the cost
   * of a slight delay on the first operation.
   *
   * @param {boolean} lazy - If true, delays Web Worker and WebAssembly initialization
   *                         until needed. Default is false.
   *
   * @example
   * // Create a wallet with immediate initialization
   * const wallet = new FedimintWallet();
   * wallet.open();
   *
   * // Create a wallet with lazy initialization
   * const lazyWallet = new FedimintWallet(true);
   * // Some time later...
   * lazyWallet.initialize();
   * lazyWallet.open();
   */
  constructor(lazy: boolean = false) {
    this._openPromise = new Promise((resolve) => {
      this._resolveOpen = resolve
    })
    this._client = new WorkerClient()
    this.mint = new MintService(this._client)
    this.lightning = new LightningService(this._client)
    this.balance = new BalanceService(this._client)
    this.federation = new FederationService(this._client)
    this.recovery = new RecoveryService(this._client)
    this.wallet = new WalletService(this._client)

    logger.info('FedimintWallet instantiated')

    if (!lazy) {
      this.initialize()
    }
  }

  async initialize() {
    logger.info('Initializing WorkerClient')
    await this._client.initialize()
    logger.info('WorkerClient initialized')
  }

  async waitForOpen() {
    if (this._isOpen) return Promise.resolve()
    return this._openPromise
  }

  async open(clientName: string = DEFAULT_CLIENT_NAME) {
    await this._client.initialize()
    // TODO: Determine if this should be safe or throw
    if (this._isOpen) throw new Error('The FedimintWallet is already open.')
    const { success } = await this._client.sendSingleMessage<{
      success: boolean
    }>('open', { clientName })
    if (success) {
      this._isOpen = !!success
      this._resolveOpen()
    }
    return success
  }

  async joinFederation(
    inviteCode: string,
    clientName: string = DEFAULT_CLIENT_NAME,
  ) {
    await this._client.initialize()
    // TODO: Determine if this should be safe or throw
    if (this._isOpen)
      throw new Error(
        'The FedimintWallet is already open. You can only call `joinFederation` on closed clients.',
      )
    try {
      const response = await this._client.sendSingleMessage<{
        success: boolean
      }>('join', { inviteCode, clientName })
      if (response.success) {
        this._isOpen = true
        this._resolveOpen()
      }

      return response.success
    } catch (e) {
      logger.error('Error joining federation', e)
      return false
    }
  }

  /**
   * This should ONLY be called when UNLOADING the wallet client.
   * After this call, the FedimintWallet instance should be discarded.
   */
  async cleanup() {
    this._openPromise = undefined
    this._isOpen = false
    await this._client.cleanup()
  }

  isOpen() {
    return this._isOpen
  }

  async previewFederation(inviteCode: string) {
    const response = this._client.sendSingleMessage<{
      config: FederationConfig
      federation_id: string
    }>('previewFederation', { inviteCode })
    return response
  }

  /**
   * Sets the log level for the library.
   * @param level The desired log level ('DEBUG', 'INFO', 'WARN', 'ERROR', 'NONE').
   */
  setLogLevel(level: LogLevel) {
    logger.setLevel(level)
    logger.info(`Log level set to ${level}.`)
  }

  /**
   * Parses a federation invite code and retrieves its details.
   *
   * This method sends the provided invite code to the WorkerClient for parsing.
   * The response includes the federation_id and url.
   *
   * @param {string} inviteCode - The invite code to be parsed.
   * @returns {Promise<{ federation_id: string, url: string}>}
   *          A promise that resolves to an object containing:
   *          - `federation_id`: The id of the feder.
   *          - `url`: One of the apipoints to connect to the federation
   *
   * @throws {Error} If the WorkerClient encounters an issue during the parsing process.
   *
   * @example
   * const inviteCode = "example-invite-code";
   * const parsedCode = await wallet.parseInviteCode(inviteCode);
   * console.log(parsedCode.federation_id, parsedCode.url);
   */
  async parseInviteCode(inviteCode: string) {
    const response = await this._client.sendSingleMessage<{
      type: string
      data: JSONValue
      requestId: number
    }>('parseInviteCode', { inviteCode })
    return response
  }

  /**
   * Parses a BOLT11 Lightning invoice and retrieves its details.
   *
   * This method sends the provided invoice string to the WorkerClient for parsing.
   * The response includes details such as the amount, expiry, and memo.
   *
   * @param {string} invoiceStr - The BOLT11 invoice string to be parsed.
   * @returns {Promise<{ amount: string, expiry: number, memo: string }>}
   *          A promise that resolves to an object containing:
   *          - `amount`: The amount specified in the invoice.
   *          - `expiry`: The expiry time of the invoice in seconds.
   *          - `memo`: A description or memo attached to the invoice.
   *
   * @throws {Error} If the WorkerClient encounters an issue during the parsing process.
   *
   * @example
   * const invoiceStr = "lnbc1...";
   * const parsedInvoice = await wallet.parseBolt11Invoice(invoiceStr);
   * console.log(parsedInvoice.amount, parsedInvoice.expiry, parsedInvoice.memo);
   */
  async parseBolt11Invoice(invoiceStr: string) {
    const response = await this._client.sendSingleMessage<{
      type: string
      data: JSONValue
      requestId: number
    }>('parseBolt11Invoice', { invoiceStr })
    return response
  }
}
